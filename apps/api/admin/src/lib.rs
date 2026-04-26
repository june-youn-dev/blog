//! Administrative Cloudflare Worker entry point for the blog API.

mod auth;

use axum::extract::{Path, Query, State};
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, HOST, LOCATION, ORIGIN, SET_COOKIE, STRICT_TRANSPORT_SECURITY,
    VARY,
};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use blog_api_core::db::{self, TrashResult, UpdateResult, WriteError};
use blog_api_core::dto::{
    CreatePost, ErrorResponse, FirebaseSessionRequest, Post, PostSummary, SessionIssuedResponse,
    SessionStatusResponse, UpdatePost,
};
use blog_api_core::validation::{
    validate_create_post_input, validate_revision_no, validate_slug, validate_update_post_input,
};
use serde::Deserialize;
use tower_service::Service;
use uuid::Uuid;
use worker::{Context, Env, HttpRequest, event};

use crate::auth::{
    AuthError, Authenticated, authenticate_admin_firebase_token, clear_session_cookie,
    has_valid_session_cookie, is_allowed_admin_origin, issue_session_cookie, resolved_admin_origin,
};

const ROOT_PATH: &str = "/";
const AUTH_SESSION_PATH: &str = "/auth/session";
const AUTH_FIREBASE_SESSION_PATH: &str = "/auth/firebase-session";
const ADMIN_POSTS_PATH: &str = "/admin/posts";
const POSTS_PATH: &str = "/posts";
const POSTS_BY_ID_PATH: &str = "/posts/by-id/{public_id}";
const POSTS_BY_SLUG_PATH: &str = "/posts/by-slug/{slug}";
const HSTS_VALUE: &str = "max-age=31536000; includeSubDomains; preload";
const CORS_ALLOW_METHODS: &str = "GET, POST, PUT, DELETE, OPTIONS";
const CORS_ALLOW_HEADERS: &str = "authorization, content-type";

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> worker::Result<axum::http::Response<axum::body::Body>> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_owned();
    let configured_admin_origin = match resolved_admin_origin(&env) {
        Ok(origin) => origin,
        Err(error) => return Ok(error.into_response()),
    };
    let request_origin =
        req.headers().get(ORIGIN).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let allowed_origin = request_origin
        .as_deref()
        .filter(|origin| is_allowed_admin_origin(origin, &configured_admin_origin));

    if method == Method::OPTIONS
        && is_browser_api_path(&path)
        && let Some(origin) = allowed_origin
    {
        let mut response = axum::http::Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(axum::body::Body::empty())
            .expect("CORS preflight response should be valid");
        apply_cors_headers(response.headers_mut(), origin);

        if should_attach_hsts(&uri) {
            response
                .headers_mut()
                .insert(STRICT_TRANSPORT_SECURITY, HSTS_VALUE.parse().expect("valid HSTS header"));
        }

        return Ok(response);
    }

    if let Some(location) = https_redirect_location(&uri) {
        return Ok(axum::http::Response::builder()
            .status(StatusCode::PERMANENT_REDIRECT)
            .header(LOCATION, location)
            .body(axum::body::Body::empty())
            .expect("HTTPS redirect response should be valid"));
    }

    let is_production_https = should_attach_hsts(&uri);
    let mut response = router(env).call(req).await?;

    if is_browser_api_path(&path)
        && let Some(origin) = allowed_origin
    {
        apply_cors_headers(response.headers_mut(), origin);
    }

    if is_production_https {
        response
            .headers_mut()
            .insert(STRICT_TRANSPORT_SECURITY, HSTS_VALUE.parse().expect("valid HSTS header"));
    }

    Ok(response)
}

fn https_redirect_location(uri: &Uri) -> Option<String> {
    if uri.scheme_str() != Some("http") || is_local_host(uri.host()) {
        return None;
    }

    let authority = uri.authority()?.as_str();
    let path_and_query = uri.path_and_query().map(|value| value.as_str()).unwrap_or("/");
    Some(format!("https://{authority}{path_and_query}"))
}

fn should_attach_hsts(uri: &Uri) -> bool {
    uri.scheme_str() == Some("https") && !is_local_host(uri.host())
}

fn apply_cors_headers(headers: &mut axum::http::HeaderMap, origin: &str) {
    headers.insert(
        ACCESS_CONTROL_ALLOW_ORIGIN,
        origin.parse().expect("allowed CORS origin should be a valid header value"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_CREDENTIALS,
        "true".parse().expect("valid CORS credentials header"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        CORS_ALLOW_METHODS.parse().expect("valid CORS methods header"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        CORS_ALLOW_HEADERS.parse().expect("valid CORS headers header"),
    );
    headers.insert(VARY, "Origin".parse().expect("valid Vary header"));
}

fn is_browser_api_path(path: &str) -> bool {
    path.starts_with("/auth/")
        || path == ADMIN_POSTS_PATH
        || path.starts_with("/admin/posts/")
        || path == POSTS_PATH
        || path.starts_with("/posts/")
}

fn is_local_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn router(env: Env) -> Router {
    Router::new()
        .route(ROOT_PATH, get(root))
        .route(AUTH_SESSION_PATH, get(get_session).post(create_session).delete(delete_session))
        .route(AUTH_FIREBASE_SESSION_PATH, axum::routing::post(create_firebase_session))
        .route(ADMIN_POSTS_PATH, get(list_admin_posts))
        .route(POSTS_PATH, get(list_posts).post(create_post))
        .route(POSTS_BY_ID_PATH, get(get_post_by_public_id))
        .route(POSTS_BY_SLUG_PATH, get(get_post_by_slug).put(update_post).delete(delete_post))
        .with_state(env)
}

async fn root() -> &'static str {
    "blog-api-admin: ok"
}

fn request_requires_secure_cookie(headers: &HeaderMap) -> bool {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(':').next());

    !is_local_host(host)
}

#[worker::send]
async fn get_session(
    State(env): State<Env>,
    headers: HeaderMap,
) -> Result<Json<SessionStatusResponse>, AuthError> {
    let authenticated = has_valid_session_cookie(
        &env,
        headers.get("cookie").and_then(|value| value.to_str().ok()),
    )?;

    Ok(Json(SessionStatusResponse { authenticated }))
}

#[worker::send]
async fn create_session(
    _auth: Authenticated,
    State(env): State<Env>,
    headers: HeaderMap,
) -> Result<([(axum::http::header::HeaderName, String); 1], Json<SessionIssuedResponse>), AuthError>
{
    let cookie = issue_session_cookie(&env, request_requires_secure_cookie(&headers))?;
    Ok(([(SET_COOKIE, cookie)], Json(SessionIssuedResponse { ok: true, session: "issued".into() })))
}

#[worker::send]
async fn create_firebase_session(
    State(env): State<Env>,
    headers: HeaderMap,
    Json(body): Json<FirebaseSessionRequest>,
) -> Result<([(axum::http::header::HeaderName, String); 1], Json<SessionIssuedResponse>), AuthError>
{
    authenticate_admin_firebase_token(&env, &body.id_token).await?;
    let cookie = issue_session_cookie(&env, request_requires_secure_cookie(&headers))?;
    Ok(([(SET_COOKIE, cookie)], Json(SessionIssuedResponse { ok: true, session: "issued".into() })))
}

#[worker::send]
async fn delete_session(
    headers: HeaderMap,
) -> ([(axum::http::header::HeaderName, String); 1], StatusCode) {
    (
        [(SET_COOKIE, clear_session_cookie(request_requires_secure_cookie(&headers)))],
        StatusCode::NO_CONTENT,
    )
}

#[worker::send]
async fn list_admin_posts(
    _auth: Authenticated,
    State(env): State<Env>,
) -> Result<Json<Vec<Post>>, StatusCode> {
    match db::list_all_posts(&env).await {
        Ok(posts) => Ok(Json(posts)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[worker::send]
async fn list_posts(State(env): State<Env>) -> Result<Json<Vec<PostSummary>>, StatusCode> {
    match db::list_public_posts(&env).await {
        Ok(posts) => Ok(Json(posts)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[worker::send]
async fn get_post_by_public_id(
    State(env): State<Env>,
    Path(public_id): Path<Uuid>,
) -> Result<Json<Post>, StatusCode> {
    match db::get_post_by_public_id(&env, public_id).await {
        Ok(Some(post)) => Ok(Json(post)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[worker::send]
async fn get_post_by_slug(
    State(env): State<Env>,
    Path(slug): Path<String>,
) -> Result<Json<Post>, StatusCode> {
    if validate_slug(&slug).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }

    match db::get_post_by_slug(&env, &slug).await {
        Ok(Some(post)) => Ok(Json(post)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[worker::send]
async fn create_post(
    _auth: Authenticated,
    State(env): State<Env>,
    Json(body): Json<CreatePost>,
) -> Result<(StatusCode, Json<Post>), (StatusCode, Json<ErrorResponse>)> {
    if let Err(msg) = validate_create_post_input(&body) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg.into() })));
    }

    match db::create_post(&env, &body).await {
        Ok(post) => Ok((StatusCode::CREATED, Json(post))),
        Err(WriteError::Validation(msg)) => {
            Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg })))
        }
        Err(WriteError::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "canonical slug is already in use by another post".into(),
            }),
        )),
        Err(WriteError::Internal(_)) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "internal error".into() }),
        )),
    }
}

#[worker::send]
async fn update_post(
    _auth: Authenticated,
    State(env): State<Env>,
    Path(slug): Path<String>,
    Json(body): Json<UpdatePost>,
) -> Result<Json<Post>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(msg) = validate_update_post_input(&slug, &body) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg.into() })));
    }

    match db::update_post(&env, &slug, &body).await {
        Ok(UpdateResult::Updated(post)) => Ok(Json(post)),
        Ok(UpdateResult::NotFound) => {
            Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: "post not found".into() })))
        }
        Ok(UpdateResult::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse { error: "revision_no mismatch".into() }),
        )),
        Err(WriteError::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "canonical slug is already in use by another post".into(),
            }),
        )),
        Err(WriteError::Validation(msg)) => {
            Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg })))
        }
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "internal error".into() }),
        )),
    }
}

#[derive(Deserialize)]
struct DeleteParams {
    revision_no: i64,
}

#[worker::send]
async fn delete_post(
    _auth: Authenticated,
    State(env): State<Env>,
    Path(slug): Path<String>,
    Query(params): Query<DeleteParams>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if let Err(msg) = validate_slug(&slug) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg.into() })));
    }
    if let Err(msg) = validate_revision_no(params.revision_no) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(ErrorResponse { error: msg.into() })));
    }

    match db::trash_post(&env, &slug, params.revision_no).await {
        Ok(TrashResult::Trashed) => Ok(StatusCode::NO_CONTENT),
        Ok(TrashResult::NotFound) => {
            Err((StatusCode::NOT_FOUND, Json(ErrorResponse { error: "post not found".into() })))
        }
        Ok(TrashResult::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse { error: "revision_no mismatch".into() }),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: "internal error".into() }),
        )),
    }
}
