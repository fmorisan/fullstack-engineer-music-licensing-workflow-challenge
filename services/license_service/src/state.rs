//! Shared application state.

use std::sync::Arc;

use sqlx::PgPool;

use crate::upstream::Upstream;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    upstream: Upstream,
}

impl AppState {
    /// Assemble the state.
    pub fn new(pool: PgPool, upstream: Upstream) -> Self {
        Self {
            inner: Arc::new(Inner { pool, upstream }),
        }
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    /// Cross-service validation client.
    #[must_use]
    pub fn upstream(&self) -> &Upstream {
        &self.inner.upstream
    }
}
