//! Public Cloudflare Worker entry point for the blog API.

use axum::extract::{Path, State};
use axum::http::header::{LOCATION, STRICT_TRANSPORT_SECURITY};
use axum::http::{StatusCode, Uri};
use axum::routing::get;
use axum::{Json, Router};
use blog_api_core::db;
use blog_api_core::dto::{Post, PostSummary};
use blog_api_core::validation::validate_slug;
use tower_service::Service;
use uuid::Uuid;
use worker::{Context, Env, HttpRequest, event};

const ROOT_PATH: &str = "/";
const POSTS_PATH: &str = "/posts";
const POSTS_BY_ID_PATH: &str = "/posts/by-id/{public_id}";
const POSTS_BY_SLUG_PATH: &str = "/posts/by-slug/{slug}";
const HSTS_VALUE: &str = "max-age=31536000; includeSubDomains; preload";

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> worker::Result<axum::http::Response<axum::body::Body>> {
    let uri = req.uri().clone();

    if let Some(location) = https_redirect_location(&uri) {
        return Ok(axum::http::Response::builder()
            .status(StatusCode::PERMANENT_REDIRECT)
            .header(LOCATION, location)
            .body(axum::body::Body::empty())
            .expect("HTTPS redirect response should be valid"));
    }

    let attach_hsts = should_attach_hsts(&uri);
    let mut response = router(env).call(req).await?;

    if attach_hsts {
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

fn is_local_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn router(env: Env) -> Router {
    Router::new()
        .route(ROOT_PATH, get(root))
        .route(POSTS_PATH, get(list_posts))
        .route(POSTS_BY_ID_PATH, get(get_post_by_public_id))
        .route(POSTS_BY_SLUG_PATH, get(get_post_by_slug))
        .with_state(env)
}

async fn root() -> &'static str {
    "blog-api: ok"
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

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::{https_redirect_location, is_local_host, should_attach_hsts};

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
    fn recognizes_local_hosts() {
        assert!(is_local_host(Some("localhost")));
        assert!(is_local_host(Some("127.0.0.1")));
        assert!(!is_local_host(Some("api.example.com")));
    }
}
