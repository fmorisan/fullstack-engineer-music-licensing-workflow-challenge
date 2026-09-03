//! Shared application state.

use sqlx::PgPool;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
}

impl AppState {
    /// Assemble the state.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
