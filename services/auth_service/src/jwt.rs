//! JWT issuance and verification (HS256, per ADR-002).
//!
//! auth_service is the only issuer; verifying parties (Traefik forward-auth
//! via `/verify`, and services re-checking service-to-service calls) share
//! the `JWT_SECRET`.

use chrono::{DateTime, Utc};
use jsonwebtoken::Algorithm;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use licensing_core::Role;
use uuid::Uuid;

/// JWT claims issued by auth_service.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    /// Subject: the authenticated user id.
    pub sub: String,
    /// The user's role.
    pub role: Role,
    /// The user's organization id, when the role belongs to one.
    pub org_id: Option<Uuid>,
    /// Issued-at, unix seconds.
    pub iat: i64,
    /// Expiry, unix seconds.
    pub exp: i64,
}

/// Mint a signed token.
///
/// # Errors
///
/// Fails on JWT encoding errors (e.g. malformed secret).
pub fn encode(
    secret: &str,
    user_id: Uuid,
    role: Role,
    org_id: Option<Uuid>,
    issued_at: DateTime<Utc>,
    ttl_secs: i64,
) -> anyhow::Result<String> {
    let claims = Claims {
        sub: user_id.to_string(),
        role,
        org_id,
        iat: issued_at.timestamp(),
        exp: issued_at.timestamp() + ttl_secs,
    };
    let token = jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

/// Validate a token and return its claims.
///
/// # Errors
///
/// Fails for invalid signatures, malformed tokens, and expired `exp` claims.
pub fn decode(secret: &str, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
}

/// Extract the bearer token from an `Authorization` header value.
#[must_use]
pub fn bearer_token(authorization: &str) -> Option<&str> {
    authorization
        .strip_prefix("Bearer ")
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret";

    fn claims_now() -> (Uuid, Role, Option<Uuid>) {
        (
            licensing_core::new_id(),
            Role::Studio,
            Some(licensing_core::new_id()),
        )
    }

    #[test]
    fn encode_decode_round_trips() {
        let (user_id, role, org_id) = claims_now();
        let token = encode(SECRET, user_id, role, org_id, Utc::now(), 60).unwrap();
        let claims = decode(SECRET, &token).unwrap();
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.role, Role::Studio);
        assert_eq!(claims.org_id, org_id);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn expired_tokens_are_rejected() {
        // Well past jsonwebtoken's default 60s clock-skew leeway.
        let (user_id, role, org_id) = claims_now();
        let past = Utc::now() - chrono::Duration::seconds(600);
        let token = encode(SECRET, user_id, role, org_id, past, 60).unwrap();
        assert!(decode(SECRET, &token).is_err());
    }

    #[test]
    fn foreign_secret_is_rejected() {
        let (user_id, role, org_id) = claims_now();
        let token = encode(SECRET, user_id, role, org_id, Utc::now(), 60).unwrap();
        assert!(decode("other-secret", &token).is_err());
    }

    #[test]
    fn tampered_tokens_are_rejected() {
        let (user_id, role, org_id) = claims_now();
        let mut token = encode(SECRET, user_id, role, org_id, Utc::now(), 60).unwrap();
        token.replace_range(0..1, "X");
        assert!(decode(SECRET, &token).is_err());
    }

    #[test]
    fn bearer_parsing_is_strict() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token(""), None);
    }
}
