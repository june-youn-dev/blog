//! Bearer-token authentication for write endpoints.
//!
//! The token is stored as a Cloudflare Worker Secret named `API_KEY`.
//! Handlers that need authentication extract [`Authenticated`] from the
//! request — the extractor rejects requests with a missing, malformed,
//! or incorrect `Authorization` header before the handler body runs.

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use worker::{Env, Fetch, Request, RequestInit};

const SESSION_COOKIE_NAME: &str = "admin_session";
const SESSION_TTL_SECS: i64 = 60 * 60 * 12;
const FIREBASE_CERTS_URL: &str =
    "https://www.googleapis.com/robot/v1/metadata/x509/securetoken@system.gserviceaccount.com";

/// Constant-time token comparison using SHA-256 digests.
///
/// Both values are hashed to fixed-length digests before comparison,
/// preventing timing side-channels from leaking either the token
/// content or its length.
fn verify_token(submitted: &[u8], expected: &[u8]) -> bool {
    let submitted_hash = Sha256::digest(submitted);
    let expected_hash = Sha256::digest(expected);

    // Constant-time comparison of fixed-length (32-byte) digests.
    let mut diff = 0u8;
    for (a, b) in submitted_hash.iter().zip(expected_hash.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Rejection type for [`Authenticated`], returning a JSON error body
/// consistent with the `{"error": "..."}` contract used by all other
/// endpoints.
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
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

/// Proof that the request carries a valid bearer token.
///
/// This is a zero-size extractor: its only purpose is to gate access.
/// Include it in a handler's argument list to require authentication:
///
/// ```ignore
/// async fn create_post(
///     _auth: Authenticated,
///     State(env): State<Env>,
///     Json(body): Json<CreatePost>,
/// ) -> impl IntoResponse { ... }
/// ```
pub struct Authenticated;

#[derive(Debug, Clone, Deserialize)]
struct FirebaseClaims {
    aud: String,
    iss: String,
    sub: String,
    exp: usize,
    iat: usize,
    auth_time: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct FirebaseCerts(std::collections::HashMap<String, String>);

#[derive(Debug, Clone)]
pub struct FirebaseIdentity {
    pub uid: String,
}

pub fn issue_session_cookie(env: &Env, secure: bool) -> Result<String, AuthError> {
    let secret = session_secret(env)?;
    let expires_at = now_unix_secs() + SESSION_TTL_SECS;
    let payload = format!("v1.admin.{expires_at}");
    let signature = sign_value(&payload, &secret);
    let token = format!("{payload}.{signature}");
    let secure_attr = if secure { "; Secure" } else { "" };

    Ok(format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; Max-Age={SESSION_TTL_SECS}; SameSite=Lax{secure_attr}"
    ))
}

pub fn clear_session_cookie(secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; Max-Age=0; SameSite=Lax{secure_attr}")
}

pub fn has_valid_session_cookie(env: &Env, cookie_header: Option<&str>) -> Result<bool, AuthError> {
    let Some(cookie_header) = cookie_header else {
        return Ok(false);
    };

    let Some(token) = parse_cookie(cookie_header, SESSION_COOKIE_NAME) else {
        return Ok(false);
    };

    let secret = session_secret(env)?;
    Ok(verify_session(&token, &secret))
}

pub async fn verify_firebase_id_token(
    env: &Env,
    id_token: &str,
) -> Result<FirebaseIdentity, AuthError> {
    let project_id = env
        .var("FIREBASE_PROJECT_ID")
        .map(|v| v.to_string())
        .map_err(|_| AuthError::internal("missing FIREBASE_PROJECT_ID"))?;

    let header = decode_header(id_token)
        .map_err(|_| AuthError::bad_request("invalid Firebase ID token header"))?;

    if header.alg != Algorithm::RS256 {
        return Err(AuthError::unauthorized("unsupported Firebase token algorithm"));
    }

    let kid = header.kid.ok_or_else(|| AuthError::unauthorized("missing Firebase token key id"))?;

    let certs = fetch_firebase_certs().await?;
    let pem =
        certs.0.get(&kid).ok_or_else(|| AuthError::unauthorized("unknown Firebase signing key"))?;

    let decoding_key = DecodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|_| AuthError::internal("failed to parse Firebase public key"))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[project_id.as_str()]);
    validation.set_issuer(&[format!("https://securetoken.google.com/{project_id}")]);

    let token = decode::<FirebaseClaims>(id_token, &decoding_key, &validation)
        .map_err(|_| AuthError::unauthorized("invalid Firebase ID token"))?;

    let claims = token.claims;
    let now = now_unix_secs() as usize;

    if claims.sub.trim().is_empty() {
        return Err(AuthError::unauthorized("Firebase token subject is empty"));
    }
    if claims.aud != project_id {
        return Err(AuthError::unauthorized("Firebase token audience mismatch"));
    }
    if claims.iss != format!("https://securetoken.google.com/{project_id}") {
        return Err(AuthError::unauthorized("Firebase token issuer mismatch"));
    }
    if claims.iat > now {
        return Err(AuthError::unauthorized("Firebase token issued-at time is invalid"));
    }
    if claims.auth_time > now {
        return Err(AuthError::unauthorized("Firebase auth_time is invalid"));
    }
    if claims.exp <= now {
        return Err(AuthError::unauthorized("Firebase token is expired"));
    }

    if !is_admin_uid(env, &claims.sub)? {
        return Err(AuthError::forbidden("Firebase user is not authorized for admin access"));
    }

    Ok(FirebaseIdentity { uid: claims.sub })
}

fn session_secret(env: &Env) -> Result<String, AuthError> {
    env.secret("ADMIN_SESSION_SECRET")
        .map(|s| s.to_string())
        .map_err(|_| AuthError::internal("missing ADMIN_SESSION_SECRET"))
}

fn now_unix_secs() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        (worker::js_sys::Date::now() / 1000.0).floor() as i64
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

fn sign_value(payload: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(payload.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(secret.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use core::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let mut pair = part.trim().splitn(2, '=');
        let key = pair.next()?.trim();
        let value = pair.next()?.trim();
        (key == name).then(|| value.to_owned())
    })
}

fn is_admin_uid(env: &Env, uid: &str) -> Result<bool, AuthError> {
    let allowed_uid = env
        .var("ADMIN_FIREBASE_UID")
        .map(|v| v.to_string())
        .map_err(|_| AuthError::internal("missing ADMIN_FIREBASE_UID"))?;
    let allowed_uid = allowed_uid.trim();

    if allowed_uid.is_empty() {
        return Err(AuthError::internal("empty ADMIN_FIREBASE_UID"));
    }

    Ok(verify_token(uid.as_bytes(), allowed_uid.as_bytes()))
}

async fn fetch_firebase_certs() -> Result<FirebaseCerts, AuthError> {
    let request = Request::new_with_init(FIREBASE_CERTS_URL, &RequestInit::new())
        .map_err(|_| AuthError::internal("failed to build Firebase cert request"))?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| AuthError::internal("failed to fetch Firebase signing keys"))?;

    if response.status_code() != 200 {
        return Err(AuthError::internal("Firebase signing keys endpoint failed"));
    }

    let cert_map = response
        .json::<std::collections::HashMap<String, String>>()
        .await
        .map_err(|_| AuthError::internal("invalid Firebase signing keys response"))?;

    Ok(FirebaseCerts(cert_map))
}

fn verify_session(token: &str, secret: &str) -> bool {
    let mut parts = token.split('.');
    let version = parts.next();
    let subject = parts.next();
    let expires_at = parts.next();
    let signature = parts.next();

    if parts.next().is_some() || version != Some("v1") || subject != Some("admin") {
        return false;
    }

    let Some(expires_at) = expires_at else {
        return false;
    };
    let Some(signature) = signature else {
        return false;
    };

    let Ok(expires_at) = expires_at.parse::<i64>() else {
        return false;
    };

    if expires_at <= now_unix_secs() {
        return false;
    }

    let payload = format!("v1.admin.{expires_at}");
    let expected = sign_value(&payload, secret);
    verify_token(signature.as_bytes(), expected.as_bytes())
}

impl FromRequestParts<Env> for Authenticated {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &Env) -> Result<Self, Self::Rejection> {
        let expected_api_key = state
            .secret("API_KEY")
            .map(|s| s.to_string())
            .map_err(|_| AuthError::internal("missing API_KEY"))?;

        if let Some(cookie_header) = parts.headers.get("cookie").and_then(|v| v.to_str().ok())
            && has_valid_session_cookie(state, Some(cookie_header))?
        {
            return Ok(Authenticated);
        }

        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AuthError::unauthorized("unauthorized"))?;

        // Parse "Bearer <token>" — case-insensitive scheme per RFC 6750.
        let token = header
            .get(7..)
            .filter(|_| header[..7].eq_ignore_ascii_case("bearer "))
            .ok_or_else(|| AuthError::unauthorized("unauthorized"))?;

        if verify_token(token.as_bytes(), expected_api_key.as_bytes()) {
            Ok(Authenticated)
        } else {
            Err(AuthError::unauthorized("unauthorized"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SESSION_COOKIE_NAME, now_unix_secs, parse_cookie, sign_value, verify_session};

    #[test]
    fn parses_named_cookie() {
        let header = "foo=bar; admin_session=token-value; theme=dark";
        assert_eq!(parse_cookie(header, SESSION_COOKIE_NAME).as_deref(), Some("token-value"));
    }

    #[test]
    fn accepts_valid_session_signature() {
        let expires_at = now_unix_secs() + 600;
        let payload = format!("v1.admin.{expires_at}");
        let secret = "session-secret";
        let token = format!("{payload}.{}", sign_value(&payload, secret));
        assert!(verify_session(&token, secret));
    }

    #[test]
    fn rejects_expired_session() {
        let expires_at = now_unix_secs() - 1;
        let payload = format!("v1.admin.{expires_at}");
        let secret = "session-secret";
        let token = format!("{payload}.{}", sign_value(&payload, secret));
        assert!(!verify_session(&token, secret));
    }
}
