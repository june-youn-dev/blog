//! Firebase ID token verification for the single-admin browser flow.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use worker::{Env, Fetch, Request, RequestInit};

use super::AuthError;
use crate::auth::session::now_unix_secs;

const FIREBASE_CERTS_URL: &str =
    "https://www.googleapis.com/robot/v1/metadata/x509/securetoken@system.gserviceaccount.com";

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

pub async fn authenticate_admin_firebase_token(env: &Env, id_token: &str) -> Result<(), AuthError> {
    let claims = decode_verified_firebase_claims(env, id_token).await?;
    let now = now_unix_secs() as usize;
    let project_id = firebase_project_id(env)?;

    validate_registered_firebase_claims(&claims, &project_id, now)?;
    ensure_admin_uid(env, &claims.sub)?;

    Ok(())
}

fn firebase_project_id(env: &Env) -> Result<String, AuthError> {
    env.var("FIREBASE_PROJECT_ID")
        .map(|value| value.to_string())
        .map_err(|_| AuthError::internal("missing FIREBASE_PROJECT_ID"))
}

async fn decode_verified_firebase_claims(
    env: &Env,
    id_token: &str,
) -> Result<FirebaseClaims, AuthError> {
    let project_id = firebase_project_id(env)?;
    let key_id = decode_verified_key_id(id_token)?;
    let decoding_key = fetch_firebase_decoding_key(&key_id).await?;
    let validation = firebase_validation(&project_id);

    let token = decode::<FirebaseClaims>(id_token, &decoding_key, &validation)
        .map_err(|_| AuthError::unauthorized("invalid Firebase ID token"))?;

    Ok(token.claims)
}

fn decode_verified_key_id(id_token: &str) -> Result<String, AuthError> {
    let header = decode_header(id_token)
        .map_err(|_| AuthError::bad_request("invalid Firebase ID token header"))?;

    if header.alg != Algorithm::RS256 {
        return Err(AuthError::unauthorized("unsupported Firebase token algorithm"));
    }

    header.kid.ok_or_else(|| AuthError::unauthorized("missing Firebase token key id"))
}

fn firebase_validation(project_id: &str) -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[project_id]);
    validation.set_issuer(&[format!("https://securetoken.google.com/{project_id}")]);
    validation
}

fn validate_registered_firebase_claims(
    claims: &FirebaseClaims,
    project_id: &str,
    now: usize,
) -> Result<(), AuthError> {
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

    Ok(())
}

fn ensure_admin_uid(env: &Env, uid: &str) -> Result<(), AuthError> {
    let allowed_uid = env
        .var("ADMIN_FIREBASE_UID")
        .map(|value| value.to_string())
        .map_err(|_| AuthError::internal("missing ADMIN_FIREBASE_UID"))?;
    let allowed_uid = allowed_uid.trim();

    if allowed_uid.is_empty() {
        return Err(AuthError::internal("empty ADMIN_FIREBASE_UID"));
    }

    if uid == allowed_uid {
        Ok(())
    } else {
        Err(AuthError::forbidden("Firebase user is not authorized for admin access"))
    }
}

async fn fetch_firebase_decoding_key(key_id: &str) -> Result<DecodingKey, AuthError> {
    let certs = fetch_firebase_certs().await?;
    let pem = certs
        .0
        .get(key_id)
        .ok_or_else(|| AuthError::unauthorized("unknown Firebase signing key"))?;

    DecodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|_| AuthError::internal("failed to parse Firebase public key"))
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

#[cfg(test)]
mod tests {
    use super::{FirebaseClaims, validate_registered_firebase_claims};

    fn claims() -> FirebaseClaims {
        FirebaseClaims {
            aud: "demo-project".into(),
            iss: "https://securetoken.google.com/demo-project".into(),
            sub: "firebase-uid".into(),
            exp: 200,
            iat: 100,
            auth_time: 100,
        }
    }

    #[test]
    fn accepts_valid_registered_claims() {
        assert!(validate_registered_firebase_claims(&claims(), "demo-project", 150).is_ok());
    }

    #[test]
    fn rejects_empty_subject() {
        let mut claims = claims();
        claims.sub = " ".into();
        assert!(validate_registered_firebase_claims(&claims, "demo-project", 150).is_err());
    }

    #[test]
    fn rejects_future_iat() {
        let mut claims = claims();
        claims.iat = 151;
        assert!(validate_registered_firebase_claims(&claims, "demo-project", 150).is_err());
    }

    #[test]
    fn rejects_expired_token() {
        let mut claims = claims();
        claims.exp = 150;
        assert!(validate_registered_firebase_claims(&claims, "demo-project", 150).is_err());
    }
}
