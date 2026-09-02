//! Access-token fixtures for driving service routers in tests.

use licensing_core::Role;
use licensing_core::new_id;
use uuid::Uuid;

use crate::keys::RsaKeyPair;

/// Mint a valid RS256 access token for the given keypair.
///
/// # Panics
///
/// Panics if signing fails; tests cannot proceed without tokens.
#[must_use]
pub fn access_token(pair: &RsaKeyPair, role: Role, org_id: Option<Uuid>) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = platform::jwt::Claims {
        sub: new_id().to_string(),
        role,
        org_id,
        iss: platform::jwt::ISSUER.to_string(),
        iat: now,
        exp: now + 900,
    };
    platform::jwt::encode(&pair.private_pem, &claims).expect("token signing failed")
}

/// Convenience: a STUDIO token bound to a fresh organization.
#[must_use]
pub fn studio_token(pair: &RsaKeyPair) -> String {
    access_token(pair, Role::Studio, Some(new_id()))
}

/// Convenience: a LABEL token bound to a fresh organization.
#[must_use]
pub fn label_token(pair: &RsaKeyPair) -> String {
    access_token(pair, Role::Label, Some(new_id()))
}
