//! Shared application state.

use std::sync::Arc;

use elasticsearch::Elasticsearch;
use redis::aio::ConnectionManager;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    es: Elasticsearch,
    redis: ConnectionManager,
    search_cache_ttl_secs: u64,
    detail_cache_ttl_secs: u64,
}

impl AppState {
    /// Assemble the state.
    pub fn new(
        es: Elasticsearch,
        redis: ConnectionManager,
        search_cache_ttl_secs: u64,
        detail_cache_ttl_secs: u64,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                es,
                redis,
                search_cache_ttl_secs,
                detail_cache_ttl_secs,
            }),
        }
    }

    /// ElasticSearch client.
    #[must_use]
    pub fn es(&self) -> &Elasticsearch {
        &self.inner.es
    }

    /// Redis connection.
    #[must_use]
    pub fn redis(&self) -> &ConnectionManager {
        &self.inner.redis
    }

    /// Search-results cache TTL, seconds.
    #[must_use]
    pub fn search_cache_ttl_secs(&self) -> u64 {
        self.inner.search_cache_ttl_secs
    }

    /// Song-detail cache TTL, seconds.
    #[must_use]
    pub fn detail_cache_ttl_secs(&self) -> u64 {
        self.inner.detail_cache_ttl_secs
    }
}
