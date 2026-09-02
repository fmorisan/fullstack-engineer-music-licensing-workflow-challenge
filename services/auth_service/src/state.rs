//! Shared application state.

use std::sync::Arc;

use sqlx::PgPool;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    jwt_secret: String,
    jwt_expiry_secs: i64,
}

impl AppState {
    /// Assemble the state.
    pub fn new(pool: PgPool, jwt_secret: String, jwt_expiry_secs: i64) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                jwt_secret,
                jwt_expiry_secs,
            }),
        }
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    /// JWT signing secret.
    #[must_use]
    pub fn jwt_secret(&self) -> &str {
        &self.inner.jwt_secret
    }

    /// Token lifetime in seconds.
    #[must_use]
    pub fn jwt_expiry_secs(&self) -> i64 {
        self.inner.jwt_expiry_secs
    }
}
