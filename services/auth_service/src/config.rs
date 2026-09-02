//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Default access-token lifetime: 15 minutes (ADR-012).
const DEFAULT_JWT_EXPIRY_SECS: i64 = 900;

/// Default HTTP port for auth_service.
pub const DEFAULT_PORT: u16 = 8101;

/// Runtime configuration for auth_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string (auth_db).
    pub database_url: String,
    /// Path to the PKCS#8 private key PEM used for RS256 signing.
    pub jwt_private_key_file: String,
    /// Access-token lifetime in seconds.
    pub jwt_expiry_secs: i64,
    /// HTTP listen port.
    pub port: u16,
}

impl Config {
    /// Load configuration from the environment.
    ///
    /// # Errors
    ///
    /// Fails when `DATABASE_URL` or `JWT_PRIVATE_KEY_FILE` is missing.
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context(
            "DATABASE_URL is required (e.g. postgres://acme:acme_dev_only@localhost:5433/auth_db)",
        )?;
        let jwt_private_key_file = std::env::var("JWT_PRIVATE_KEY_FILE").context(
            "JWT_PRIVATE_KEY_FILE is required (PKCS#8 PEM; see infrastructure/docker/keys)",
        )?;
        let jwt_expiry_secs = std::env::var("JWT_EXPIRY_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_JWT_EXPIRY_SECS);
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Ok(Self {
            database_url,
            jwt_private_key_file,
            jwt_expiry_secs,
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_constants_are_sane() {
        assert_eq!(DEFAULT_PORT, 8101);
        assert_eq!(DEFAULT_JWT_EXPIRY_SECS, 900);
    }
}
