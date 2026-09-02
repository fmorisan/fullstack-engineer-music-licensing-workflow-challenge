//! Shared application state.

use std::sync::Arc;

use sqlx::PgPool;

use crate::keys::SigningKeys;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    signing: SigningKeys,
    jwt_expiry_secs: i64,
}

impl AppState {
    /// Assemble the state.
    pub fn new(pool: PgPool, signing: SigningKeys, jwt_expiry_secs: i64) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                signing,
                jwt_expiry_secs,
            }),
        }
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    /// Active signing keys.
    #[must_use]
    pub fn signing(&self) -> &SigningKeys {
        &self.inner.signing
    }

    /// Access-token lifetime in seconds.
    #[must_use]
    pub fn jwt_expiry_secs(&self) -> i64 {
        self.inner.jwt_expiry_secs
    }
}
