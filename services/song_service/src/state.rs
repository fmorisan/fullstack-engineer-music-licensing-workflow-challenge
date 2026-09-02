//! Shared application state.

use std::sync::Arc;

use sqlx::PgPool;

use platform::MediaPresigner;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    media: MediaPresigner,
}

impl AppState {
    /// Assemble the state.
    pub fn new(pool: PgPool, media: MediaPresigner) -> Self {
        Self {
            inner: Arc::new(Inner { pool, media }),
        }
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    /// Pre-signed media client.
    #[must_use]
    pub fn media(&self) -> &MediaPresigner {
        &self.inner.media
    }
}
