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
    refresh_ttl_secs: i64,
    cookie_secure: bool,
}

impl AppState {
    /// Assemble the state.
    pub fn new(
        pool: PgPool,
        signing: SigningKeys,
        jwt_expiry_secs: i64,
        refresh_ttl_secs: i64,
        cookie_secure: bool,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                signing,
                jwt_expiry_secs,
                refresh_ttl_secs,
                cookie_secure,
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

    /// Refresh-token lifetime in seconds.
    #[must_use]
    pub fn refresh_ttl_secs(&self) -> i64 {
        self.inner.refresh_ttl_secs
    }

    /// Whether cookies carry the `Secure` attribute (production TLS).
    #[must_use]
    pub fn cookie_secure(&self) -> bool {
        self.inner.cookie_secure
    }
}
