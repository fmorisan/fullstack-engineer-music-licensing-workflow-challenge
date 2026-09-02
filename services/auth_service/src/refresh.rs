//! Refresh token material: generation and hashing (ADR-012).

use rand_core::RngCore;
use sha2::Digest;

/// Cookie carrying the refresh token; scoped to /auth paths.
pub const COOKIE_NAME: &str = "refresh_token";

/// Generate a fresh opaque refresh token: 32 random bytes, base64url.
#[must_use]
pub fn generate() -> String {
    use base64::Engine;
    let mut bytes = [0_u8; 32];
    rand_core::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 hex digest of a raw token; the only form stored server-side.
#[must_use]
pub fn hash(raw: &str) -> String {
    let digest = sha2::Sha256::digest(raw.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_url_safe_and_unique() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), 43, "32 bytes base64url without padding");
        assert_ne!(a, b);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "cookie-safe alphabet only"
        );
    }

    #[test]
    fn hashes_are_stable_and_hex() {
        assert_eq!(hash("abc"), hash("abc"));
        assert_ne!(hash("abc"), hash("abd"));
        assert_eq!(hash("abc").len(), 64);
        assert!(hash("abc").chars().all(|c| c.is_ascii_hexdigit()));
    }
}
