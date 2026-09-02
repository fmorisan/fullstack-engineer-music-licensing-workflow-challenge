//! Argon2id password hashing (pure Rust, OS-provided randomness).

use argon2::Argon2;
use argon2::password_hash::PasswordHash;
use argon2::password_hash::PasswordHasher;
use argon2::password_hash::PasswordVerifier;
use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;

/// Hash a password with Argon2id and a fresh random salt.
///
/// # Errors
///
/// Fails if the underlying Argon2 parameters cannot be applied (e.g. password
/// longer than the hash input limit).
pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let encoded = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| anyhow::anyhow!("argon2 hashing failed: {err}"))?
        .to_string();
    Ok(encoded)
}

/// Verify a password against an Argon2id hash.
///
/// Returns `false` for wrong passwords and malformed stored hashes alike;
/// verification failure never distinguishes the two to callers.
#[must_use]
pub fn verify(password: &str, encoded: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_round_trips() {
        let encoded = hash("correct horse battery staple").unwrap();
        assert!(encoded.starts_with("$argon2id$"));
        assert!(verify("correct horse battery staple", &encoded));
    }

    #[test]
    fn wrong_password_fails() {
        let encoded = hash("correct horse battery staple").unwrap();
        assert!(!verify("tr0ub4dor&3", &encoded));
    }

    #[test]
    fn malformed_hash_fails_closed() {
        assert!(!verify("anything", "not-a-hash"));
        assert!(!verify("anything", ""));
    }

    #[test]
    fn salts_are_random_per_call() {
        let a = hash("same password").unwrap();
        let b = hash("same password").unwrap();
        assert_ne!(a, b);
    }
}
