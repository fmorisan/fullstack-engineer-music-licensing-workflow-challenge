//! License-event consumption: map events to notifications and persist them
//! idempotently (ADR-006).
//!
//! Delivery is at-least-once; the `source_event_id` unique constraint makes
//! replays no-ops. Live fan-out and channel delivery are composed around
//! [`apply_event`] by the consumer loop.

use licensing_core::LicenseEvent;
use licensing_core::LicenseEventKind;
use licensing_core::LicenseSnapshot;
use licensing_core::LicenseState;
use licensing_core::Role;
use licensing_core::topics;
use platform::EventConsumer;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::NotificationRow;

/// Consumer group id; a fresh group replays the topic to backfill the inbox
/// from retained history.
pub const GROUP_ID: &str = "notification-service";

/// Notification types surfaced to clients (and email subjects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    /// A studio made (or re-made) an offer — the label is notified.
    OfferReceived,
    /// A label countered — the studio is notified.
    CounterOfferReceived,
    /// The current deal was accepted — the counterparty is notified.
    OfferAccepted,
    /// The deal was rejected — the counterparty is notified.
    OfferRejected,
}

impl NotificationType {
    /// Wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationType::OfferReceived => "OFFER_RECEIVED",
            NotificationType::CounterOfferReceived => "COUNTER_OFFER_RECEIVED",
            NotificationType::OfferAccepted => "OFFER_ACCEPTED",
            NotificationType::OfferRejected => "OFFER_REJECTED",
        }
    }
}

/// Derive the notification type and recipient org from the event.
///
/// The counterparty of the acting side receives the notification: a studio
/// actor notifies the label org, and vice versa (`actor_role` on the
/// snapshot; see licensing-core).
#[must_use]
pub fn classify(event: &LicenseEvent) -> Option<(NotificationType, Uuid)> {
    let license: &LicenseSnapshot = &event.license;
    let recipient_org = if license.actor_role == Role::Studio {
        license.label_id
    } else {
        license.studio_id
    };
    let kind = match (event.kind, license.from_state, license.state) {
        (LicenseEventKind::Created, _, LicenseState::Offer)
        | (LicenseEventKind::StateChanged, Some(LicenseState::CounterOffer), LicenseState::Offer) => {
            NotificationType::OfferReceived
        }
        (LicenseEventKind::StateChanged, Some(LicenseState::Offer), LicenseState::CounterOffer) => {
            NotificationType::CounterOfferReceived
        }
        (LicenseEventKind::StateChanged, _, LicenseState::Accepted) => {
            NotificationType::OfferAccepted
        }
        (LicenseEventKind::StateChanged, _, LicenseState::Rejected) => {
            NotificationType::OfferRejected
        }
        // Unknown or nonsensical shapes (e.g. CREATED with a non-OFFER
        // state) are skipped rather than guessed.
        _ => return None,
    };
    Some((kind, recipient_org))
}

/// Persist the notification for an event; `Ok(None)` when the event maps to
/// no notification or was already applied (idempotent replay).
///
/// # Errors
///
/// Propagates database errors.
pub async fn apply_event(
    pool: &PgPool,
    event: &LicenseEvent,
) -> sqlx::Result<Option<NotificationRow>> {
    let Some((kind, recipient_org)) = classify(event) else {
        return Ok(None);
    };

    let id = licensing_core::new_id();
    let payload = serde_json::json!({
        "event_id": event.event_id.to_string(),
        "kind": event.kind,
        "license": event.license,
    });

    let row = sqlx::query_as::<_, NotificationRow>(
        "INSERT INTO notifications
            (id, recipient_org_id, type, payload, source_event_id)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (source_event_id) DO NOTHING
         RETURNING id, recipient_user_id, recipient_org_id, type, payload,
                   source_event_id, read_at, created_at",
    )
    .bind(id)
    .bind(recipient_org)
    .bind(kind.as_str())
    .bind(&payload)
    .bind(event.event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Build the catalog consumer.
///
/// # Errors
///
/// Fails when the Kafka client cannot be constructed.
pub fn consumer(bootstrap: &str) -> anyhow::Result<EventConsumer> {
    Ok(EventConsumer::new(
        bootstrap,
        GROUP_ID,
        &[topics::LICENSE_EVENTS],
    )?)
}

/// Run the consumer loop forever: apply events, skipping malformed payloads
/// without stalling.
///
/// # Errors
///
/// Returns only on unrecoverable consumer failures.
pub async fn run(consumer: EventConsumer, pool: PgPool) -> anyhow::Result<()> {
    loop {
        let message = consumer
            .recv_timeout(std::time::Duration::from_secs(1))
            .await?;
        let Some(message) = message else {
            continue;
        };
        match serde_json::from_slice::<LicenseEvent>(&message.payload) {
            Ok(event) => {
                if let Err(err) = apply_event(&pool, &event).await {
                    tracing::warn!(%err, "notification apply failed; will not retry this event");
                }
            }
            Err(err) => {
                tracing::warn!(%err, "malformed license event skipped");
            }
        }
    }
}
