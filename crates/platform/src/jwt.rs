//! RS256 JWT issuance and verification (ADR-011/012).
//!
//! auth_service is the only issuer; the gateway (Kong) and every service
//! verify with the public key. Key ids are derived deterministically from the
//! key material so all parties agree without coordination.

use jsonwebtoken::Algorithm;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use licensing_core::Role;
use rsa::pkcs8::DecodePrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::pkcs8::LineEnding;
use sha2::Digest;
use uuid::Uuid;

/// Issuer claim every platform token carries; matches the Kong credential key.
pub const ISSUER: &str = "acme-auth";

/// JWT claims issued by auth_service.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    /// Subject: the authenticated user id.
    pub sub: String,
    /// The user's role.
    pub role: Role,
    /// The user's organization id, when the role belongs to one.
    pub org_id: Option<Uuid>,
    /// Issuer; must equal [`ISSUER`].
    pub iss: String,
    /// Issued-at, unix seconds.
    pub iat: i64,
    /// Expiry, unix seconds.
    pub exp: i64,
}

/// Errors from key or token operations.
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    /// The provided key material could not be parsed.
    #[error("invalid key material: {0}")]
    Key(String),
    /// The token failed validation (signature, expiry, issuer, shape).
    #[error("token validation failed: {0}")]
    Token(#[from] jsonwebtoken::errors::Error),
}

fn parse_private(private_pem: &str) -> Result<rsa::RsaPrivateKey, JwtError> {
    rsa::RsaPrivateKey::from_pkcs8_pem(private_pem)
        .map_err(|err| JwtError::Key(format!("bad private key: {err}")))
}

/// Derive the public key PEM (SubjectPublicKeyInfo) from a PKCS#8 private
/// key PEM.
///
/// # Errors
///
/// Fails when the private key cannot be parsed.
pub fn public_pem_from_private(private_pem: &str) -> Result<String, JwtError> {
    let private = parse_private(private_pem)?;
    private
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|err| JwtError::Key(format!("public key encode failed: {err}")))
}

/// Deterministic key id for a public key PEM: first 16 hex chars of its
/// SHA-256 digest. All parties derive the same `kid` from the same key.
#[must_use]
pub fn key_id(public_pem: &str) -> String {
    use std::fmt::Write as _;
    let digest = sha2::Sha256::digest(public_pem.as_bytes());
    let mut kid = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        let _ = write!(kid, "{byte:02x}");
    }
    kid
}

/// Mint an RS256 token; the `kid` header is derived from the signing key.
///
/// # Errors
///
/// Fails on unparseable key material or JWT encoding errors.
pub fn encode(private_pem: &str, claims: &Claims) -> Result<String, JwtError> {
    let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes())
        .map_err(|err| JwtError::Key(format!("bad private key: {err}")))?;
    let kid = key_id(&public_pem_from_private(private_pem)?);
    let header = Header {
        alg: Algorithm::RS256,
        kid: Some(kid),
        ..Default::default()
    };
    jsonwebtoken::encode(&header, claims, &encoding).map_err(Into::into)
}

/// Validate an RS256 token against the public key; enforces `exp` and issuer.
///
/// # Errors
///
/// Fails on bad keys, bad signatures, expired tokens, and foreign issuers.
pub fn decode(public_pem: &str, token: &str) -> Result<Claims, JwtError> {
    let decoding = DecodingKey::from_rsa_pem(public_pem.as_bytes())
        .map_err(|err| JwtError::Key(format!("bad public key: {err}")))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[ISSUER]);
    let data = jsonwebtoken::decode::<Claims>(token, &decoding, &validation)?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Keys {
        private: String,
        public: String,
        other: test_support::RsaKeyPair,
    }

    fn keys() -> Keys {
        let pair = test_support::RsaKeyPair::generate();
        Keys {
            public: pair.public_pem.clone(),
            private: pair.private_pem,
            other: test_support::RsaKeyPair::generate(),
        }
    }

    fn sample_claims() -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims {
            sub: licensing_core::new_id().to_string(),
            role: Role::Studio,
            org_id: Some(licensing_core::new_id()),
            iss: ISSUER.to_string(),
            iat: now,
            exp: now + 900,
        }
    }

    #[test]
    fn round_trips_and_carries_kid_header() {
        let keys = keys();
        let claims = sample_claims();
        let token = encode(&keys.private, &claims).unwrap();
        assert_eq!(decode(&keys.public, &token).unwrap(), claims);

        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some(key_id(&keys.public).as_str()));
    }

    #[test]
    fn public_derivation_matches_generated_public() {
        let keys = keys();
        let derived = public_pem_from_private(&keys.private).unwrap();
        assert_eq!(derived.trim(), keys.public.trim());
    }

    #[test]
    fn rejects_wrong_key_expired_and_foreign_issuer() {
        let keys = keys();
        let claims = sample_claims();
        let token = encode(&keys.private, &claims).unwrap();

        assert!(decode(&keys.other.public_pem, &token).is_err());

        let past = Claims {
            iat: claims.iat - 3600,
            exp: claims.exp - 3600,
            ..claims.clone()
        };
        let expired = encode(&keys.private, &past).unwrap();
        assert!(decode(&keys.public, &expired).is_err());

        let foreign = Claims {
            iss: "someone-else".into(),
            ..claims
        };
        let forged = encode(&keys.private, &foreign).unwrap();
        assert!(decode(&keys.public, &forged).is_err());
    }

    #[test]
    fn key_ids_differ_between_keys() {
        let keys = keys();
        assert_ne!(key_id(&keys.public), key_id(&keys.other.public_pem));
    }
}
