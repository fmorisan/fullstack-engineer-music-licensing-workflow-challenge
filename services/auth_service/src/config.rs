//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Fallback secret when `JWT_SECRET` is unset; development ergonomics only.
const DEV_JWT_SECRET: &str = "dev-only-ephemeral-secret";

/// Default token lifetime: 24 hours.
const DEFAULT_JWT_EXPIRY_SECS: i64 = 86_400;

/// Default HTTP port for auth_service.
pub const DEFAULT_PORT: u16 = 8101;

/// Runtime configuration for auth_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string (auth_db).
    pub database_url: String,
    /// HS256 signing secret, shared with verifying services (ADR-002).
    pub jwt_secret: String,
    /// Token lifetime in seconds.
    pub jwt_expiry_secs: i64,
    /// HTTP listen port.
    pub port: u16,
}

impl Config {
    /// Load configuration from the environment.
    ///
    /// # Errors
    ///
    /// Fails when `DATABASE_URL` is missing.
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context(
            "DATABASE_URL is required (e.g. postgres://acme:acme_dev_only@localhost:5433/auth_db)",
        )?;
        let jwt_secret = match std::env::var("JWT_SECRET") {
            Ok(s) if !s.is_empty() => s,
            _ => {
                tracing::warn!(
                    "JWT_SECRET not set; using ephemeral default (sessions break on restart)"
                );
                DEV_JWT_SECRET.to_string()
            }
        };
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
            jwt_secret,
            jwt_expiry_secs,
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: env-var based construction is exercised through integration
    // tests; unit tests here only pin the defaults that have no env input.

    #[test]
    fn dev_constants_are_sane() {
        assert_eq!(DEFAULT_PORT, 8101);
        assert_eq!(DEFAULT_JWT_EXPIRY_SECS, 86_400);
    }
}
