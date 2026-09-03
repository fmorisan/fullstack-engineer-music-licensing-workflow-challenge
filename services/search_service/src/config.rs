//! Environment-driven configuration.

use anyhow::{Context, Result};

/// Default HTTP port for search_service.
pub const DEFAULT_PORT: u16 = 8104;

/// Runtime configuration for search_service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Kafka bootstrap servers (song.events consumer).
    pub kafka_bootstrap: String,
    /// ElasticSearch base URL.
    pub elasticsearch_url: String,
    /// Redis connection URL (hot-query cache).
    pub redis_url: String,
    /// Search-results cache TTL, seconds.
    pub search_cache_ttl_secs: u64,
    /// Song-detail cache TTL, seconds.
    pub detail_cache_ttl_secs: u64,
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
        let kafka_bootstrap = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
            .context("KAFKA_BOOTSTRAP_SERVERS is required (e.g. localhost:9092)")?;
        let elasticsearch_url = std::env::var("ELASTICSEARCH_URL")
            .context("ELASTICSEARCH_URL is required (e.g. http://localhost:9200)")?;
        let redis_url = std::env::var("REDIS_URL")
            .context("REDIS_URL is required (e.g. redis://localhost:6379)")?;
        let search_cache_ttl_secs = std::env::var("SEARCH_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        let detail_cache_ttl_secs = std::env::var("DETAIL_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        Ok(Self {
            kafka_bootstrap,
            elasticsearch_url,
            redis_url,
            search_cache_ttl_secs,
            detail_cache_ttl_secs,
            port,
        })
    }
}
