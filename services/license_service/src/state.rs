//! Shared application state.

use std::sync::Arc;

use sqlx::PgPool;

use crate::upstream::Upstream;
use platform::pubsub::Fanout;
use platform::pubsub::Publisher;

/// Cheaply cloneable state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    pool: PgPool,
    upstream: Upstream,
    publisher: Option<Publisher>,
    fanout: Option<Fanout>,
}

impl AppState {
    /// Assemble the core state; live paths (outbox-adjacent Redis
    /// publish/subscribe) attach with [`Self::with_live`].
    pub fn new(pool: PgPool, upstream: Upstream) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                upstream,
                publisher: None,
                fanout: None,
            }),
        }
    }

    /// Attach live-updates plumbing (Redis publisher + channel fanout).
    ///
    /// # Panics
    ///
    /// Panics if called after the state has been cloned — attach before
    /// serving.
    #[must_use]
    pub fn with_live(mut self, publisher: Publisher, fanout: Fanout) -> Self {
        let inner = Arc::get_mut(&mut self.inner)
            .expect("with_live must be called before cloning the state");
        inner.publisher = Some(publisher);
        inner.fanout = Some(fanout);
        self
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

    /// Live publisher, when wired.
    #[must_use]
    pub fn publisher(&self) -> Option<&Publisher> {
        self.inner.publisher.as_ref()
    }

    /// Channel fanout for the SSE stream, when wired.
    #[must_use]
    pub fn fanout(&self) -> Option<&Fanout> {
        self.inner.fanout.as_ref()
    }
}
