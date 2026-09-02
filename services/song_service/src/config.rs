//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Default HTTP port for song_service.
pub const DEFAULT_PORT: u16 = 8103;

/// Runtime configuration for song_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection string (song_db).
    pub database_url: String,
    /// Kafka bootstrap servers for the outbox relay (ADR-004).
    pub kafka_bootstrap: String,
    /// S3/MinIO endpoint for pre-signed uploads.
    pub s3_endpoint: String,
    /// Static S3 access key.
    pub s3_access_key: String,
    /// Static S3 secret key.
    pub s3_secret_key: String,
    /// Bucket for box art and audio previews.
    pub s3_bucket: String,
    /// Pre-signed URL lifetime in seconds.
    pub presign_expiry_secs: u64,
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
            "DATABASE_URL is required (e.g. postgres://acme:acme_dev_only@localhost:5433/song_db)",
        )?;
        let kafka_bootstrap = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
            .context("KAFKA_BOOTSTRAP_SERVERS is required (e.g. localhost:9092)")?;
        let s3_endpoint = std::env::var("S3_ENDPOINT")
            .context("S3_ENDPOINT is required (e.g. http://localhost:9000)")?;
        let s3_access_key = std::env::var("S3_ACCESS_KEY").context("S3_ACCESS_KEY is required")?;
        let s3_secret_key = std::env::var("S3_SECRET_KEY").context("S3_SECRET_KEY is required")?;
        let s3_bucket =
            std::env::var("S3_BUCKET_SONG_MEDIA").unwrap_or_else(|_| "song-media".into());
        let presign_expiry_secs = std::env::var("PRESIGN_EXPIRY_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Ok(Self {
            database_url,
            kafka_bootstrap,
            s3_endpoint,
            s3_access_key,
            s3_secret_key,
            s3_bucket,
            presign_expiry_secs,
            port,
        })
    }
}
