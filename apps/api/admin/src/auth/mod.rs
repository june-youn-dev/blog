//! Authentication policy and request extraction for administrative endpoints.

mod firebase;
mod session;

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::header::{ORIGIN, REFERER};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use subtle::ConstantTimeEq;
use worker::Env;

pub use self::firebase::authenticate_admin_firebase_token;
pub use self::session::{clear_session_cookie, has_valid_session_cookie, issue_session_cookie};

pub(crate) const LOCAL_ADMIN_ORIGINS: &[&str] = &[
    "http://localhost:8080",
    "http://127.0.0.1:8080",
    "http://localhost:8081",
    "http://127.0.0.1:8081",
];

pub(crate) fn secrets_match(submitted: &[u8], expected: &[u8]) -> bool {
    submitted.ct_eq(expected).into()
}

#[derive(Debug)]
pub struct AuthError {
    status: StatusCode,
    message: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl AuthError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub(crate) fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

pub struct Authenticated;

pub(crate) fn resolved_admin_origin(env: &Env) -> Result<String, AuthError> {
    let raw = env
        .var("ADMIN_ORIGIN")
        .ok()
        .map(|value| value.to_string())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    let Some(origin) = raw else {
        return Err(AuthError::internal("missing ADMIN_ORIGIN for deployed admin access"));
    };

    normalize_admin_origin(&origin)
}

fn normalize_admin_origin(origin: &str) -> Result<String, AuthError> {
    let uri: axum::http::Uri = origin
        .parse()
        .map_err(|_| AuthError::internal("ADMIN_ORIGIN must be a valid absolute origin"))?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| AuthError::internal("ADMIN_ORIGIN must include a scheme"))?;
    let authority =
        uri.authority().ok_or_else(|| AuthError::internal("ADMIN_ORIGIN must include a host"))?;

    if uri.path() != "/" || uri.path_and_query().and_then(|value| value.query()).is_some() {
        return Err(AuthError::internal(
            "ADMIN_ORIGIN must be an origin only, without a path or query",
        ));
    }

    Ok(format!("{scheme}://{authority}"))
}

pub(crate) fn is_allowed_admin_origin(origin: &str, configured_origin: &str) -> bool {
    LOCAL_ADMIN_ORIGINS.contains(&origin) || configured_origin == origin
}

fn session_cookie_request_is_allowed(parts: &Parts, configured_origin: &str) -> bool {
    if let Some(origin) = parts.headers.get(ORIGIN).and_then(|value| value.to_str().ok()) {
        return is_allowed_admin_origin(origin, configured_origin);
    }

    if let Some(referer) = parts.headers.get(REFERER).and_then(|value| value.to_str().ok()) {
        return LOCAL_ADMIN_ORIGINS
            .iter()
            .any(|origin| referer == *origin || referer.starts_with(&format!("{origin}/")))
            || referer == configured_origin
            || referer.starts_with(&format!("{configured_origin}/"));
    }

    false
}

impl FromRequestParts<Env> for Authenticated {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &Env) -> Result<Self, Self::Rejection> {
        let expected_api_key = state
            .secret("API_KEY")
            .map(|secret| secret.to_string())
            .map_err(|_| AuthError::internal("missing API_KEY"))?;
        let configured_admin_origin = resolved_admin_origin(state)?;

        if let Some(cookie_header) =
            parts.headers.get("cookie").and_then(|value| value.to_str().ok())
            && has_valid_session_cookie(state, Some(cookie_header))?
        {
            if !session_cookie_request_is_allowed(parts, &configured_admin_origin) {
                return Err(AuthError::forbidden(
                    "browser origin is not authorized for admin access",
                ));
            }
            return Ok(Authenticated);
        }

        let header = parts
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AuthError::unauthorized("unauthorized"))?;

        let token = header
            .get(7..)
            .filter(|_| header[..7].eq_ignore_ascii_case("bearer "))
            .ok_or_else(|| AuthError::unauthorized("unauthorized"))?;

        if secrets_match(token.as_bytes(), expected_api_key.as_bytes()) {
            Ok(Authenticated)
        } else {
            Err(AuthError::unauthorized("unauthorized"))
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::{
        LOCAL_ADMIN_ORIGINS, is_allowed_admin_origin, normalize_admin_origin,
        session_cookie_request_is_allowed,
    };

    #[test]
    fn recognizes_allowed_admin_origins() {
        assert!(is_allowed_admin_origin("http://localhost:8080", "https://admin.example.com"));
        assert!(is_allowed_admin_origin("https://admin.example.com", "https://admin.example.com",));
        assert!(!is_allowed_admin_origin("https://evil.example.com", "https://admin.example.com",));
    }

    #[test]
    fn local_admin_origin_allowlist_is_self_consistent() {
        for origin in LOCAL_ADMIN_ORIGINS {
            assert!(is_allowed_admin_origin(origin, "https://admin.example.com"));
        }
    }

    #[test]
    fn accepts_session_cookie_requests_with_allowed_origin() {
        let request = Request::builder()
            .header("origin", "http://localhost:8080")
            .body(())
            .expect("request should build");
        let (parts, _) = request.into_parts();

        assert!(session_cookie_request_is_allowed(&parts, "https://admin.example.com"));
    }

    #[test]
    fn accepts_session_cookie_requests_with_allowed_referer() {
        let request = Request::builder()
            .header("referer", "https://admin.example.com/admin/")
            .body(())
            .expect("request should build");
        let (parts, _) = request.into_parts();

        assert!(session_cookie_request_is_allowed(&parts, "https://admin.example.com"));
    }

    #[test]
    fn rejects_session_cookie_requests_without_browser_provenance() {
        let request = Request::builder().body(()).expect("request should build");
        let (parts, _) = request.into_parts();

        assert!(!session_cookie_request_is_allowed(&parts, "https://admin.example.com"));
    }

    #[test]
    fn normalizes_admin_origin_without_trailing_slash() {
        assert_eq!(
            normalize_admin_origin("https://admin.example.com/").expect("origin should normalize"),
            "https://admin.example.com"
        );
    }
}
