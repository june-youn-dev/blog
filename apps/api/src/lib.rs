//! Cloudflare Worker entry point for the blog API.
//!
//! The crate is organized around a narrow three-layer split:
//!
//! - this file (`lib.rs`) is the HTTP edge — the `#[event(fetch)]` entry
//!   point, the [`axum::Router`], and the handlers that translate
//!   between HTTP and repository calls;
//! - [`dto`] holds every data-transfer object that crosses the HTTP
//!   boundary, derives `serde` and `ts_rs::TS` so that the same shapes
//!   are reused by the static site;
//! - [`db`] is a thin repository over the Cloudflare D1 binding and is
//!   the only place in the crate that sees raw SQL;
//! - [`auth`] provides bearer-token authentication for write endpoints.
//!
//! Handlers in this file are intentionally undocumented at the item
//! level — their contract is "the route they are attached to" and they
//! are not part of any consumable API surface. The items worth reading
//! for contracts are the types in [`dto`] and the functions in [`db`].

mod auth;
mod db;
mod dto;
mod validation;

use axum::extract::{Path, Query, State};
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, HOST, LOCATION, ORIGIN, SET_COOKIE, STRICT_TRANSPORT_SECURITY,
    VARY,
};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use tower_service::Service;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use worker::{Context, Env, HttpRequest, event};

use crate::auth::{
    AuthError, Authenticated, clear_session_cookie, has_valid_session_cookie, issue_session_cookie,
    verify_firebase_id_token,
};
use crate::db::{TrashResult, UpdateResult, WriteError};
use crate::dto::{
    CreatePost, ErrorResponse, FirebaseSessionRequest, Post, PostSummary, SessionIssuedResponse,
    SessionStatusResponse, UpdatePost,
};
use crate::validation::{
    validate_create_post_input, validate_revision_no, validate_slug, validate_update_post_input,
};

const DOCS_UI_PATH: &str = "/docs";
const OPENAPI_JSON_PATH: &str = "/openapi.json";
const ROOT_PATH: &str = "/";
const AUTH_SESSION_PATH: &str = "/auth/session";
const AUTH_FIREBASE_SESSION_PATH: &str = "/auth/firebase-session";
const ADMIN_POSTS_PATH: &str = "/admin/posts";
const POSTS_PATH: &str = "/posts";
const POSTS_BY_ID_PATH: &str = "/posts/by-id/{public_id}";
const POSTS_BY_SLUG_PATH: &str = "/posts/by-slug/{slug}";
#[cfg(test)]
const API_ROUTE_PATHS: &[&str] = &[
    ROOT_PATH,
    AUTH_SESSION_PATH,
    AUTH_FIREBASE_SESSION_PATH,
    ADMIN_POSTS_PATH,
    POSTS_PATH,
    POSTS_BY_ID_PATH,
    POSTS_BY_SLUG_PATH,
];
const HSTS_VALUE: &str = "max-age=31536000; includeSubDomains; preload";
const CORS_ALLOW_METHODS: &str = "GET, POST, PUT, DELETE, OPTIONS";
const CORS_ALLOW_HEADERS: &str = "authorization, content-type";
const LOCAL_ADMIN_ORIGINS: &[&str] = &[
    "http://localhost:8080",
    "http://127.0.0.1:8080",
    "http://localhost:8081",
    "http://127.0.0.1:8081",
];

#[derive(OpenApi)]
#[openapi(
    info(
        title = "blog-api",
        version = "0.0.0",
        description = "OpenAPI description for the blog Worker API."
    ),
    paths(
        root,
        list_admin_posts,
        list_posts,
        get_post_by_public_id,
        get_post_by_slug,
        get_session,
        create_session,
        create_firebase_session,
        delete_session,
        create_post,
        update_post,
        delete_post
    ),
    components(schemas(
        Post,
        PostSummary,
        CreatePost,
        UpdatePost,
        FirebaseSessionRequest,
        crate::dto::PostStatus,
        ErrorResponse,
        SessionIssuedResponse,
        SessionStatusResponse
    ))
)]
struct ApiDoc;

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> worker::Result<axum::http::Response<axum::body::Body>> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_owned();
    let request_origin =
        req.headers().get(ORIGIN).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let allowed_origin = allowed_admin_origin(&env, request_origin.as_deref());

    if method == Method::OPTIONS
        && is_browser_api_path(&path)
        && let Some(origin) = allowed_origin.as_deref()
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
        && let Some(origin) = allowed_origin.as_deref()
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
    headers.insert(VARY, ORIGIN.as_str().parse().expect("valid Vary header"));
}

fn allowed_admin_origin(env: &Env, origin: Option<&str>) -> Option<String> {
    let origin = origin?;

    if is_local_admin_origin(origin) {
        return Some(origin.to_owned());
    }

    env.var("ADMIN_ORIGIN").ok().map(|value| value.to_string()).filter(|allowed| origin == allowed)
}

fn is_local_admin_origin(origin: &str) -> bool {
    LOCAL_ADMIN_ORIGINS.contains(&origin)
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
        .merge(SwaggerUi::new(DOCS_UI_PATH).url(OPENAPI_JSON_PATH, ApiDoc::openapi()))
        .route(ROOT_PATH, get(root))
        .route(AUTH_SESSION_PATH, get(get_session).post(create_session).delete(delete_session))
        .route(AUTH_FIREBASE_SESSION_PATH, axum::routing::post(create_firebase_session))
        .route(ADMIN_POSTS_PATH, get(list_admin_posts))
        .route(POSTS_PATH, get(list_posts).post(create_post))
        .route(POSTS_BY_ID_PATH, get(get_post_by_public_id))
        .route(POSTS_BY_SLUG_PATH, get(get_post_by_slug).put(update_post).delete(delete_post))
        .with_state(env)
}

#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = 200, description = "Simple liveness response.", body = String)
    )
)]
async fn root() -> &'static str {
    "blog-api: ok"
}

fn request_requires_secure_cookie(headers: &HeaderMap) -> bool {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(':').next());

    !is_local_host(host)
}

#[utoipa::path(
    get,
    path = "/auth/session",
    responses(
        (status = 200, description = "Administrative session-cookie status.", body = SessionStatusResponse),
        (status = 500, description = "Session secret is misconfigured.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    post,
    path = "/auth/session",
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 200, description = "Admin session cookie issued.", body = SessionIssuedResponse),
        (status = 401, description = "Unauthorized.", body = ErrorResponse),
        (status = 500, description = "Session secret is misconfigured.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    post,
    path = "/auth/firebase-session",
    request_body = FirebaseSessionRequest,
    responses(
        (status = 200, description = "Admin session cookie issued from a verified Firebase ID token.", body = SessionIssuedResponse),
        (status = 400, description = "Malformed request or malformed Firebase token.", body = ErrorResponse),
        (status = 401, description = "Firebase token verification failed.", body = ErrorResponse),
        (status = 403, description = "Firebase user is authenticated but not an authorized admin.", body = ErrorResponse),
        (status = 500, description = "Server-side Firebase configuration error.", body = ErrorResponse)
    )
)]
#[worker::send]
async fn create_firebase_session(
    State(env): State<Env>,
    headers: HeaderMap,
    Json(body): Json<FirebaseSessionRequest>,
) -> Result<([(axum::http::header::HeaderName, String); 1], Json<SessionIssuedResponse>), AuthError>
{
    let identity = verify_firebase_id_token(&env, &body.id_token).await?;
    let _ = identity.uid;
    let cookie = issue_session_cookie(&env, request_requires_secure_cookie(&headers))?;
    Ok(([(SET_COOKIE, cookie)], Json(SessionIssuedResponse { ok: true, session: "issued".into() })))
}

#[utoipa::path(
    delete,
    path = "/auth/session",
    responses(
        (status = 204, description = "Admin session cookie cleared.")
    )
)]
#[worker::send]
async fn delete_session(
    headers: HeaderMap,
) -> ([(axum::http::header::HeaderName, String); 1], StatusCode) {
    (
        [(SET_COOKIE, clear_session_cookie(request_requires_secure_cookie(&headers)))],
        StatusCode::NO_CONTENT,
    )
}

#[utoipa::path(
    get,
    path = "/admin/posts",
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 200, description = "Authenticated administrative post listing.", body = [Post]),
        (status = 401, description = "Unauthorized.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    get,
    path = "/posts",
    responses(
        (status = 200, description = "Public post listing.", body = [PostSummary]),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
#[worker::send]
async fn list_posts(State(env): State<Env>) -> Result<Json<Vec<PostSummary>>, StatusCode> {
    match db::list_public_posts(&env).await {
        Ok(posts) => Ok(Json(posts)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[utoipa::path(
    get,
    path = "/posts/by-id/{public_id}",
    params(
        ("public_id" = uuid::Uuid, Path, description = "Stable public post identifier.")
    ),
    responses(
        (status = 200, description = "Public post detail fetched by stable identifier.", body = Post),
        (status = 404, description = "Post was not found.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    get,
    path = "/posts/by-slug/{slug}",
    params(
        ("slug" = String, Path, description = "Public post slug.")
    ),
    responses(
        (status = 200, description = "Public post detail.", body = Post),
        (status = 404, description = "Post was not found.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    post,
    path = "/posts",
    request_body = CreatePost,
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 201, description = "Post created.", body = Post),
        (status = 401, description = "Unauthorized.", body = ErrorResponse),
        (status = 409, description = "The canonical slug is already in use by another post.", body = ErrorResponse),
        (status = 422, description = "Validation failed.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    put,
    path = "/posts/by-slug/{slug}",
    request_body = UpdatePost,
    params(
        ("slug" = String, Path, description = "Post slug.")
    ),
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 200, description = "Post updated.", body = Post),
        (status = 401, description = "Unauthorized.", body = ErrorResponse),
        (status = 404, description = "Post was not found.", body = ErrorResponse),
        (status = 409, description = "Revision mismatch.", body = ErrorResponse),
        (status = 422, description = "Validation failed.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

/// Query parameters for `DELETE /posts/by-slug/{slug}`.
#[derive(Deserialize)]
struct DeleteParams {
    revision_no: i64,
}

#[utoipa::path(
    delete,
    path = "/posts/by-slug/{slug}",
    params(
        ("slug" = String, Path, description = "Post slug."),
        ("revision_no" = i64, Query, description = "Optimistic concurrency token.")
    ),
    security(
        ("api_key" = [])
    ),
    responses(
        (status = 204, description = "Post moved to the trash."),
        (status = 401, description = "Unauthorized.", body = ErrorResponse),
        (status = 404, description = "Post was not found.", body = ErrorResponse),
        (status = 409, description = "Revision mismatch.", body = ErrorResponse),
        (status = 500, description = "Internal error.", body = ErrorResponse)
    )
)]
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use axum::http::{HeaderMap, Uri};
    use utoipa::OpenApi;

    use super::{
        API_ROUTE_PATHS, ApiDoc, DOCS_UI_PATH, LOCAL_ADMIN_ORIGINS, OPENAPI_JSON_PATH,
        https_redirect_location, is_browser_api_path, is_local_admin_origin,
        request_requires_secure_cookie, should_attach_hsts,
    };

    #[test]
    fn api_routes_are_unique() {
        let mut seen = HashSet::new();

        for route in API_ROUTE_PATHS {
            assert!(seen.insert(*route), "duplicate application route path: {route}");
        }
    }

    #[test]
    fn api_routes_do_not_overlap_with_docs_routes() {
        let reserved = [DOCS_UI_PATH, OPENAPI_JSON_PATH];

        for route in API_ROUTE_PATHS {
            assert!(
                !reserved.contains(route),
                "application route path overlaps a reserved documentation route: {route}"
            );
        }
    }

    #[test]
    fn openapi_documented_paths_match_runtime_api_paths() {
        let openapi = ApiDoc::openapi();
        let documented_paths: HashSet<_> = openapi.paths.paths.keys().map(String::as_str).collect();
        let expected_paths: HashSet<_> = API_ROUTE_PATHS.iter().copied().collect();

        assert_eq!(documented_paths, expected_paths);
    }

    #[test]
    fn redirects_insecure_non_local_requests_to_https() {
        let uri: Uri = "http://api.example.com/posts?draft=1".parse().expect("valid URI");

        assert_eq!(
            https_redirect_location(&uri).as_deref(),
            Some("https://api.example.com/posts?draft=1")
        );
    }

    #[test]
    fn does_not_redirect_local_http_requests() {
        let uri: Uri = "http://localhost:8787/posts".parse().expect("valid URI");

        assert!(https_redirect_location(&uri).is_none());
    }

    #[test]
    fn attaches_hsts_only_to_non_local_https_requests() {
        let production_uri: Uri = "https://api.example.com/posts".parse().expect("valid URI");
        let local_uri: Uri = "https://localhost:8787/posts".parse().expect("valid URI");

        assert!(should_attach_hsts(&production_uri));
        assert!(!should_attach_hsts(&local_uri));
    }

    #[test]
    fn recognizes_browser_api_paths() {
        assert!(is_browser_api_path("/auth/firebase-session"));
        assert!(is_browser_api_path("/admin/posts"));
        assert!(is_browser_api_path("/posts"));
        assert!(is_browser_api_path("/posts/by-slug/example"));
        assert!(!is_browser_api_path("/docs"));
    }

    #[test]
    fn local_admin_origins_match_allowlist() {
        for origin in LOCAL_ADMIN_ORIGINS {
            assert!(is_local_admin_origin(origin));
        }

        assert!(!is_local_admin_origin("https://blog.example.com"));
    }

    #[test]
    fn local_requests_receive_non_secure_cookie_attributes() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "localhost:8787".parse().expect("valid host header"));

        assert!(!request_requires_secure_cookie(&headers));
    }

    #[test]
    fn production_requests_receive_secure_cookie_attributes() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "api.example.com".parse().expect("valid host header"));

        assert!(request_requires_secure_cookie(&headers));
    }
}
