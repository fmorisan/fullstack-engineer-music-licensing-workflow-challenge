//! Shared application state.

use std::sync::Arc;

use platform::pubsub::Fanout;
use sqlx::PgPool;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
    fanout: Option<Arc<Fanout>>,
}

impl AppState {
    /// Assemble the core state; the live SSE path attaches with
    /// [`Self::with_live`].
    pub fn new(pool: PgPool) -> Self {
        Self { pool, fanout: None }
    }

    /// Attach the notifications fanout for the SSE stream.
    ///
    /// # Panics
    ///
    /// Panics if called after the state has been cloned.
    #[must_use]
    pub fn with_live(mut self, fanout: Fanout) -> Self {
        self.fanout = Some(Arc::new(fanout));
        self
    }

    /// Database pool.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Channel fanout for the SSE stream, when wired.
    #[must_use]
    pub fn fanout(&self) -> Option<&Fanout> {
        self.fanout.as_deref()
    }
}
