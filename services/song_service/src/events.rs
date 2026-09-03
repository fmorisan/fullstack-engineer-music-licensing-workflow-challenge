//! Outbox emission of song events (ADR-004).
//!
//! Events are serialized from the `licensing-core` contracts and enqueued in
//! the SAME transaction as the domain mutation; the platform relay publishes
//! them to the `song.events` topic, hydrating the search catalog.

use chrono::Utc;
use licensing_core::SongEventKind;
use licensing_core::topics;
use sqlx::PgExecutor;

use crate::models::SongRow;

async fn enqueue_event<'e, E>(executor: E, kind: SongEventKind, row: &SongRow) -> sqlx::Result<()>
where
    E: PgExecutor<'e>,
{
    let event_id = licensing_core::new_id();
    let event = licensing_core::SongEvent {
        event_id,
        kind,
        occurred_at: Utc::now(),
        song_id: row.id,
        song: Some(row.to_record()),
    };
    // Song events serialize by construction; a failure is a programming
    // error worth panicking on rather than silently dropping.
    let payload = serde_json::to_value(&event).expect("song event serializes");
    platform::outbox::enqueue(
        executor,
        event_id,
        topics::SONG_EVENTS,
        &row.id.to_string(),
        &payload,
    )
    .await
}

/// Enqueue a `CREATED` event within the caller's transaction.
///
/// # Errors
///
/// Propagates database errors.
pub async fn enqueue_created<'e, E>(executor: E, row: &SongRow) -> sqlx::Result<()>
where
    E: PgExecutor<'e>,
{
    enqueue_event(executor, SongEventKind::Created, row).await
}

/// Enqueue an `UPDATED` event within the caller's transaction.
///
/// # Errors
///
/// Propagates database errors.
pub async fn enqueue_updated<'e, E>(executor: E, row: &SongRow) -> sqlx::Result<()>
where
    E: PgExecutor<'e>,
{
    enqueue_event(executor, SongEventKind::Updated, row).await
}
