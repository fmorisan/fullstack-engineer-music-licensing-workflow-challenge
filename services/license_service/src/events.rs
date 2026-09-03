//! Outbox emission and live publication of license events (ADR-004/005).
//!
//! One post-transition [`LicenseSnapshot`] feeds two paths: the durable
//! `LicenseEvent` envelope enqueued in the same transaction as the mutation
//! (Kafka → notification_service and any other consumer), and — after
//! commit — a Redis PubSub publish for connected `/licenses/stream` clients.

use chrono::Utc;
use licensing_core::LicenseEvent;
use licensing_core::LicenseEventKind;
use licensing_core::LicenseSnapshot;
use licensing_core::LicenseState;
use licensing_core::channels;
use licensing_core::topics;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::models::LicenseRow;

/// Build the post-transition snapshot shared by both emission paths.
#[must_use]
pub fn snapshot(
    row: &LicenseRow,
    from_state: Option<LicenseState>,
    actor_user_id: Uuid,
    license_log_id: Uuid,
) -> LicenseSnapshot {
    LicenseSnapshot {
        license_id: row.id,
        movie_id: row.movie_id,
        scene_number: row.scene_number,
        song_id: row.song_id,
        actor_user_id,
        studio_id: row.studio_id,
        label_id: row.label_id,
        from_state,
        state: row.domain_state(),
        license_fee_cents: row.license_fee_cents,
        start_time_seconds: u32::try_from(row.start_time_seconds).unwrap_or(0),
        end_time_seconds: u32::try_from(row.end_time_seconds).unwrap_or(0),
        license_log_id,
    }
}

/// Enqueue a license event within the caller's transaction.
///
/// # Errors
///
/// Propagates database errors.
///
/// # Panics
///
/// Panics if the snapshot cannot be serialized — impossible for
/// `LicenseSnapshot` by construction.
pub async fn enqueue<'e, E>(
    executor: E,
    kind: LicenseEventKind,
    snapshot: &LicenseSnapshot,
) -> sqlx::Result<()>
where
    E: PgExecutor<'e>,
{
    let event = LicenseEvent {
        event_id: licensing_core::new_id(),
        kind,
        occurred_at: Utc::now(),
        license: snapshot.clone(),
    };
    let payload = serde_json::to_value(&event).expect("license event serializes");
    platform::outbox::enqueue(
        executor,
        event.event_id,
        topics::LICENSE_EVENTS,
        &snapshot.license_id.to_string(),
        &payload,
    )
    .await
}

/// Publish the post-commit snapshot to connected live streams.
///
/// Failures are logged, not fatal: the durable record lives on Kafka; live
/// clients also refetch on reconnect (ADR-005).
pub async fn publish_live(state: &crate::state::AppState, snapshot: &LicenseSnapshot) {
    let Some(publisher) = state.publisher() else {
        return;
    };
    let mut payload = serde_json::to_value(snapshot).unwrap_or_default();
    payload["event"] = serde_json::json!("license-updated");
    if let Err(err) = publisher.publish(channels::LICENSE_UPDATES, &payload).await {
        tracing::warn!(%err, "live publish failed");
    }
}
