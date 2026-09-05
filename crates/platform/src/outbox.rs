//! Transactional outbox (ADR-004).
//!
//! Services commit domain mutations and outbox rows in ONE database
//! transaction; the relay then claims unpublished rows
//! (`FOR UPDATE SKIP LOCKED`), produces them to Kafka, and marks them
//! published inside the claiming transaction. Delivery is at-least-once:
//! a crash between produce and commit republishes; consumers must be
//! idempotent.
//!
//! The expected table shape (created by each service's migrations):
//!
//! ```sql
//! CREATE TABLE outbox (
//!     id UUID PRIMARY KEY,
//!     topic TEXT NOT NULL,
//!     key TEXT NOT NULL,
//!     payload JSONB NOT NULL,
//!     created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
//!     published_at TIMESTAMPTZ
//! );
//! CREATE INDEX outbox_unpublished_idx ON outbox (id) WHERE published_at IS NULL;
//! ```
//!
//! Queries here are runtime-checked (not `query!` macros): platform is
//! infrastructure glue shared by services whose schemas live elsewhere.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;
use sqlx::PgPool;
use uuid::Uuid;

use crate::kafka::EventProducer;

/// Default relay poll cadence.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Default rows claimed per tick.
const DEFAULT_BATCH_SIZE: i64 = 100;

/// Published rows older than this are purged during ticks.
const PURGE_AFTER_DAYS: i32 = 7;

/// An unpublished outbox row as claimed by the relay.
#[derive(Debug, sqlx::FromRow)]
struct OutboxRow {
    /// Row id; also the Kafka message key tiebreaker for ordering.
    id: Uuid,
    /// Destination topic.
    topic: String,
    /// Partition key (the aggregate id).
    key: String,
    /// JSON envelope.
    payload: serde_json::Value,
}

/// Enqueue an event within the caller's transaction.
///
/// Call this in the same transaction as the domain mutation it describes
/// (ADR-004).
///
/// # Errors
///
/// Propagates database errors.
pub async fn enqueue<'e, E>(
    executor: E,
    id: Uuid,
    topic: &str,
    key: &str,
    payload: &serde_json::Value,
) -> sqlx::Result<()>
where
    E: PgExecutor<'e>,
{
    sqlx::query("INSERT INTO outbox (id, topic, key, payload) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(topic)
        .bind(key)
        .bind(payload)
        .execute(executor)
        .await?;
    Ok(())
}

/// Relay publishing committed outbox rows to Kafka.
pub struct OutboxRelay {
    producer: EventProducer,
    poll_interval: Duration,
    batch_size: i64,
}

impl OutboxRelay {
    /// Build a relay with default cadence for the given bootstrap servers.
    ///
    /// # Errors
    ///
    /// Fails when the Kafka producer cannot be constructed.
    pub fn new(bootstrap_servers: &str) -> anyhow::Result<Self> {
        Ok(Self {
            producer: EventProducer::new(bootstrap_servers)?,
            poll_interval: DEFAULT_POLL_INTERVAL,
            batch_size: DEFAULT_BATCH_SIZE,
        })
    }

    /// Override the poll cadence and batch size.
    #[must_use]
    pub fn with_cadence(mut self, poll_interval: Duration, batch_size: i64) -> Self {
        self.poll_interval = poll_interval;
        self.batch_size = batch_size;
        self
    }

    /// Claim up to one batch of unpublished rows, produce them, mark them
    /// published — all inside one transaction. Returns the number published.
    ///
    /// # Errors
    ///
    /// Database or delivery failures roll the claim back, leaving rows
    /// unpublished for the next tick.
    pub async fn run_tick(&self, pool: &PgPool) -> anyhow::Result<usize> {
        let mut tx = pool.begin().await?;
        let rows: Vec<OutboxRow> = sqlx::query_as(
            "SELECT id, topic, key, payload FROM outbox
             WHERE published_at IS NULL
             ORDER BY id
             LIMIT $1
             FOR UPDATE SKIP LOCKED",
        )
        .bind(self.batch_size)
        .fetch_all(&mut *tx)
        .await?;

        if rows.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }

        for row in &rows {
            let payload = serde_json::to_vec(&row.payload)?;
            self.producer
                .send(&row.topic, &row.key, &payload)
                .await
                .map_err(|err| anyhow::anyhow!("outbox delivery failed: {err}"))?;
            sqlx::query("UPDATE outbox SET published_at = now() WHERE id = $1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        crate::metrics::record_outbox_published(rows.len() as u64);
        Ok(rows.len())
    }

    /// Purge published rows older than [`PURGE_AFTER_DAYS`].
    ///
    /// # Errors
    ///
    /// Propagates database errors.
    pub async fn purge_published(&self, pool: &PgPool) -> sqlx::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM outbox WHERE published_at < now() - ($1 || ' days')::interval",
        )
        .bind(PURGE_AFTER_DAYS.to_string())
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Run forever: tick, purge occasionally, sleep.
    ///
    /// Tick errors are logged and retried; a down broker never stops the
    /// relay from trying again.
    ///
    /// # Errors
    /// Only returns on unrecoverable failures (pool dropped).
    pub async fn run(self, pool: PgPool) -> anyhow::Result<()> {
        let mut ticks: u64 = 0;
        loop {
            match self.run_tick(&pool).await {
                Ok(0) => {}
                Ok(count) => tracing::debug!(count, "outbox relay published rows"),
                Err(err) => tracing::warn!(%err, "outbox relay tick failed; will retry"),
            }
            ticks = ticks.wrapping_add(1);
            if ticks.is_multiple_of(240)
                && let Err(err) = self.purge_published(&pool).await
            {
                tracing::warn!(%err, "outbox purge failed");
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }
}

/// Count unpublished rows (tests and diagnostics).
///
/// # Errors
///
/// Propagates database errors.
pub async fn unpublished_count(pool: &PgPool) -> sqlx::Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE published_at IS NULL")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Read `published_at` of a row (tests and diagnostics).
///
/// # Errors
///
/// Propagates database errors; `None` when the row is gone.
pub async fn published_at(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<DateTime<Utc>>> {
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("SELECT published_at FROM outbox WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(at,)| at))
}
