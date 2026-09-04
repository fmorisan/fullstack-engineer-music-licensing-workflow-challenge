//! song.events consumer: hydrates the ElasticSearch catalog (ADR-004).
//!
//! At-least-once delivery; indexing is idempotent (document id = song id).
//! Malformed messages are logged and skipped — a poison payload must not
//! stall the consumer.

use elasticsearch::Elasticsearch;
use licensing_core::SongEvent;
use licensing_core::SongEventKind;
use licensing_core::topics;
use platform::EventConsumer;

/// Consumer group id; a fresh group replays the topic from the beginning,
/// which rehydrates an empty index from retained history.
pub const GROUP_ID: &str = "search-service";

/// Build the catalog consumer.
///
/// # Errors
///
/// Fails when the Kafka client cannot be constructed.
pub fn consumer(bootstrap: &str) -> anyhow::Result<EventConsumer> {
    Ok(EventConsumer::new(
        bootstrap,
        GROUP_ID,
        &[topics::SONG_EVENTS],
    )?)
}

/// Apply one event to the catalog.
///
/// # Errors
///
/// Propagates indexing errors.
pub async fn apply_event(client: &Elasticsearch, event: &SongEvent) -> anyhow::Result<()> {
    match event.kind {
        SongEventKind::Created | SongEventKind::Updated => {
            if let Some(song) = &event.song {
                crate::es::upsert_song(client, song).await?;
            }
        }
        SongEventKind::Deleted => {
            crate::es::delete_song(client, event.song_id).await?;
        }
    }
    Ok(())
}

/// Process one message if one arrives within `timeout`; returns whether a
/// message was processed.
///
/// # Errors
///
/// Consumer errors propagate; malformed payloads are logged and skipped
/// (counted as processed).
pub async fn poll_once(
    consumer: &EventConsumer,
    client: &Elasticsearch,
    timeout: std::time::Duration,
) -> anyhow::Result<bool> {
    let Some(message) = consumer.recv_timeout(timeout).await? else {
        return Ok(false);
    };
    match serde_json::from_slice::<SongEvent>(&message.payload) {
        Ok(event) => {
            apply_event(client, &event).await?;
            Ok(true)
        }
        Err(err) => {
            tracing::warn!(%err, "malformed song event skipped");
            Ok(true)
        }
    }
}

/// Run the consumer loop forever.
///
/// # Errors
///
/// Returns only on unrecoverable consumer/indexing failures — including
/// the zombie guard: a client that lost its group assignment (broker
/// churn) polls as eternal silence, so sustained emptiness with a zero
/// assignment bails and lets the supervisor rebuild the client.
pub async fn run(consumer: EventConsumer, client: Elasticsearch) -> anyhow::Result<()> {
    let mut empty_polls: u32 = 0;
    let mut assignment_misses: u32 = 0;
    loop {
        match poll_once(&consumer, &client, std::time::Duration::from_secs(1)).await {
            Ok(true) => {
                empty_polls = 0;
                assignment_misses = 0;
            }
            Ok(false) => {
                empty_polls = empty_polls.wrapping_add(1);
                if empty_polls.is_multiple_of(15) {
                    if consumer.assignment_count() == 0 {
                        assignment_misses += 1;
                        tracing::warn!(
                            misses = assignment_misses,
                            "indexer holds no group assignment"
                        );
                        if assignment_misses >= 2 {
                            anyhow::bail!("indexer lost its group assignment; rebuilding client");
                        }
                    } else {
                        assignment_misses = 0;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(%err, "indexer poll failed; will retry");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}
