//! Session-cookie issuance and verification.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use worker::Env;

use super::AuthError;

pub(crate) const SESSION_COOKIE_NAME: &str = "admin_session";
const SESSION_TTL_SECS: i64 = 60 * 60 * 12;
const SESSION_SUBJECT: &str = "admin_session";
const SESSION_ISSUER: &str = "blog-api";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionClaims {
    sub: String,
    iss: String,
    iat: usize,
    exp: usize,
}

/// Issues a signed administrative session cookie.
///
/// The cookie value is an `HS256`-signed JWT containing the canonical
/// issuer, subject, issued-at time, and expiration time for the
/// session. The returned string is a complete `Set-Cookie` header
/// value suitable for direct insertion into an HTTP response.
///
/// When `secure` is `true`, the cookie is marked with the `Secure`
/// attribute so that browsers only attach it over HTTPS.
///
/// # Errors
///
/// Returns [`AuthError`] when the session secret is missing or the
/// token cannot be encoded.
pub fn issue_session_cookie(env: &Env, secure: bool) -> Result<String, AuthError> {
    let secret = session_secret(env)?;
    let issued_at = now_unix_secs();
    let expires_at = issued_at + SESSION_TTL_SECS;
    let claims = SessionClaims {
        sub: SESSION_SUBJECT.into(),
        iss: SESSION_ISSUER.into(),
        iat: issued_at as usize,
        exp: expires_at as usize,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::internal("failed to issue admin session"))?;
    let secure_attr = if secure { "; Secure" } else { "" };

    Ok(format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; Max-Age={SESSION_TTL_SECS}; SameSite=Lax{secure_attr}"
    ))
}

/// Builds a `Set-Cookie` header value that clears the administrative
/// session cookie.
///
/// The returned value expires the cookie immediately and mirrors the
/// `Secure` attribute policy used when the cookie was originally
/// issued.
pub fn clear_session_cookie(secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; Max-Age=0; SameSite=Lax{secure_attr}")
}

/// Validates the administrative session cookie present in a `Cookie`
/// header.
///
/// Returns `Ok(true)` only when the named cookie is present and its
/// JWT signature, issuer, subject, and expiration time all validate
/// against the current server secret. Missing or unrelated cookies are
/// treated as `Ok(false)`.
///
/// # Errors
///
/// Returns [`AuthError`] when the server-side session secret is
/// missing.
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

pub(crate) fn now_unix_secs() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        (worker::js_sys::Date::now() / 1000.0).floor() as i64
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }
}

fn session_secret(env: &Env) -> Result<String, AuthError> {
    env.secret("ADMIN_SESSION_SECRET")
        .map(|secret| secret.to_string())
        .map_err(|_| AuthError::internal("missing ADMIN_SESSION_SECRET"))
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let mut pair = part.trim().splitn(2, '=');
        let key = pair.next()?.trim();
        let value = pair.next()?.trim();
        (key == name).then(|| value.to_owned())
    })
}

fn verify_session(token: &str, secret: &str) -> bool {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[SESSION_ISSUER]);
    validation.sub = Some(SESSION_SUBJECT.into());
    validation.validate_exp = true;
    validation.leeway = 0;
    validation.required_spec_claims =
        ["exp", "iat", "iss", "sub"].into_iter().map(str::to_owned).collect();

    decode::<SessionClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
        .is_ok_and(|decoded| decoded.claims.iat <= now_unix_secs() as usize)
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    use super::{
        SESSION_COOKIE_NAME, SESSION_ISSUER, SESSION_SUBJECT, SessionClaims, now_unix_secs,
        parse_cookie, verify_session,
    };

    fn fixture_signed_session(secret: &str, expires_at: i64) -> String {
        let issued_at = now_unix_secs();
        encode(
            &Header::new(Algorithm::HS256),
            &SessionClaims {
                sub: SESSION_SUBJECT.into(),
                iss: SESSION_ISSUER.into(),
                iat: issued_at as usize,
                exp: expires_at as usize,
            },
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("fixture session should encode")
    }

    #[test]
    fn parses_named_cookie() {
        let header = "foo=bar; admin_session=token-value; theme=dark";
        assert_eq!(parse_cookie(header, SESSION_COOKIE_NAME).as_deref(), Some("token-value"));
    }

    #[test]
    fn accepts_valid_session_signature() {
        let secret = "session-secret";
        let token = fixture_signed_session(secret, now_unix_secs() + 600);
        assert!(verify_session(&token, secret));
    }

    #[test]
    fn rejects_expired_session() {
        let secret = "session-secret";
        let token = fixture_signed_session(secret, now_unix_secs() - 1);
        assert!(!verify_session(&token, secret));
    }
}
