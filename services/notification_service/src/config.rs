//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Default HTTP port for notification_service.
pub const DEFAULT_PORT: u16 = 8106;

/// Runtime configuration for notification_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string (notification_db).
    pub database_url: String,
    /// Kafka bootstrap servers (license.events consumer).
    pub kafka_bootstrap: String,
    /// Redis URL (PubSub fan-out for the SSE stream).
    pub redis_url: String,
    /// Mailpit API base URL (EmailChannel sink).
    pub mailpit_api_url: String,
    /// HTTP listen port.
    pub port: u16,
}

impl Config {
    /// Load configuration from the environment.
    ///
    /// # Errors
    ///
    /// Fails when required variables are missing.
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context(
            "DATABASE_URL is required (e.g. postgres://acme:acme_dev_only@localhost:5433/notification_db)",
        )?;
        let kafka_bootstrap = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
            .context("KAFKA_BOOTSTRAP_SERVERS is required (e.g. localhost:9092)")?;
        let redis_url = std::env::var("REDIS_URL").context("REDIS_URL is required")?;
        let mailpit_api_url = std::env::var("MAILPIT_API_URL")
            .context("MAILPIT_API_URL is required (e.g. http://localhost:8025/api/v1)")?;
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Ok(Self {
            database_url,
            kafka_bootstrap,
            redis_url,
            mailpit_api_url,
            port,
        })
    }
}
