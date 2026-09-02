//! Signing keys and the JWKS document (ADR-011).
//!
//! The private key (PKCS#8 PEM) is supplied via `JWT_PRIVATE_KEY_FILE`; the
//! public key, `kid`, and JWKS document are derived from it. Kong embeds the
//! same public key in its declarative config; production consumers fetch
//! `/.well-known/jwks.json`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use platform::jwt;
use rsa::pkcs8::DecodePrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::pkcs8::LineEnding;
use rsa::traits::PublicKeyParts;

/// The issuer's active signing material.
#[derive(Debug, Clone)]
pub struct SigningKeys {
    /// PKCS#8 private key PEM; never leaves the service.
    pub private_pem: String,
    /// Derived SubjectPublicKeyInfo PEM.
    pub public_pem: String,
    /// Deterministic key id (see [`platform::jwt::key_id`]).
    pub kid: String,
    /// Ready-to-serve JWKS document containing the public key.
    pub jwks: serde_json::Value,
}

impl SigningKeys {
    /// Derive all material from a PKCS#8 private key PEM.
    ///
    /// # Errors
    ///
    /// Fails when the PEM cannot be parsed or encoded.
    pub fn from_private_key_pem(private_pem: &str) -> anyhow::Result<Self> {
        let private = rsa::RsaPrivateKey::from_pkcs8_pem(private_pem)?;
        let public = private.to_public_key();
        let public_pem = public.to_public_key_pem(LineEnding::LF)?.clone();
        let kid = jwt::key_id(&public_pem);

        let jwk = serde_json::json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": kid,
            "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        });
        Ok(Self {
            private_pem: private_pem.to_string(),
            public_pem,
            kid,
            jwks: serde_json::json!({ "keys": [jwk] }),
        })
    }

    /// Read the private key from a file.
    ///
    /// # Errors
    ///
    /// Fails on I/O or parse errors.
    pub fn from_private_key_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let pem = std::fs::read_to_string(path)?;
        Self::from_private_key_pem(&pem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwks_document_matches_the_signing_key() {
        let pair = test_support::RsaKeyPair::generate();
        let keys = SigningKeys::from_private_key_pem(&pair.private_pem).unwrap();

        assert_eq!(keys.public_pem.trim(), pair.public_pem.trim());
        let jwk = &keys.jwks["keys"][0];
        assert_eq!(jwk["kty"], "RSA");
        assert_eq!(jwk["alg"], "RS256");
        assert_eq!(jwk["use"], "sig");
        assert_eq!(jwk["kid"], keys.kid);

        // The token header kid matches the JWKS kid.
        let claims = platform::jwt::Claims {
            sub: licensing_core::new_id().to_string(),
            role: licensing_core::Role::Admin,
            org_id: None,
            iss: platform::jwt::ISSUER.to_string(),
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 900,
        };
        let token = platform::jwt::encode(&keys.private_pem, &claims).unwrap();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.kid.as_deref(), Some(keys.kid.as_str()));
    }
}
