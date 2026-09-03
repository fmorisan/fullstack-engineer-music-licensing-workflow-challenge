//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Default HTTP port for license_service.
pub const DEFAULT_PORT: u16 = 8105;

/// Runtime configuration for license_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string (license_db).
    pub database_url: String,
    /// Kafka bootstrap servers (license.events outbox relay).
    pub kafka_bootstrap: String,
    /// Redis URL (PubSub fan-out for the SSE stream).
    pub redis_url: String,
    /// movie_service base URL (scene/movie validation).
    pub movie_service_url: String,
    /// song_service base URL (song/label validation).
    pub song_service_url: String,
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
            "DATABASE_URL is required (e.g. postgres://acme:acme_dev_only@localhost:5433/license_db)",
        )?;
        let kafka_bootstrap = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
            .context("KAFKA_BOOTSTRAP_SERVERS is required (e.g. localhost:9092)")?;
        let redis_url = std::env::var("REDIS_URL").context("REDIS_URL is required")?;
        let movie_service_url = std::env::var("MOVIE_SERVICE_URL")
            .context("MOVIE_SERVICE_URL is required (e.g. http://localhost:8102)")?;
        let song_service_url = std::env::var("SONG_SERVICE_URL")
            .context("SONG_SERVICE_URL is required (e.g. http://localhost:8103)")?;
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Ok(Self {
            database_url,
            kafka_bootstrap,
            redis_url,
            movie_service_url,
            song_service_url,
            port,
        })
    }
}
