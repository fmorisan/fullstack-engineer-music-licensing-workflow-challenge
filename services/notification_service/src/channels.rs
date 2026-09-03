//! Delivery channels (ADR-006).
//!
//! [`NotificationChannel`] is the extension slot: `EmailChannel` ships now
//! (Mailpit locally, SMTP-capable later); push (APNs/FCM) drops in as
//! further implementations without touching the pipeline.

use std::future::Future;
use std::pin::Pin;

use crate::consumer::NotificationType;
use crate::models::NotificationRow;

/// A delivery channel for persisted notifications.
pub trait NotificationChannel: Send + Sync {
    /// Deliver one notification; failures are logged by the caller and must
    /// not panic.
    fn deliver<'a>(
        &'a self,
        notification: &'a NotificationRow,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

/// Format a fee in cents as dollars (integer math; no float precision).
fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

/// Render the notification into subject/body lines shared by channels.
#[must_use]
pub fn render(notification: &NotificationRow) -> (String, String) {
    let license = &notification.payload["license"];
    let kind = notification.notification_type.clone();
    let license_id = license["license_id"].as_str().unwrap_or("?");
    let movie = license["movie_id"].as_str().unwrap_or("?");
    let song = license["song_id"].as_str().unwrap_or("?");
    let fee = license["license_fee_cents"].as_i64().unwrap_or_default();
    let state = license["state"].as_str().unwrap_or("?");

    let subject = format!(
        "{kind}: license {prefix}…",
        prefix = &license_id.get(..8).unwrap_or(license_id)
    );
    let body = format!(
        "License {license_id} for song {song} in movie {movie} is now {state}.\n\
         Current fee: {}.\n\
         This is an automated message from the ACME licensing platform.",
        dollars(fee)
    );
    (subject, body)
}

/// Email channel: posts messages to Mailpit's HTTP API (ADR-006). The
/// `To` address is org-derived — real user addresses arrive with the user
/// directory integration.
#[derive(Clone)]
pub struct EmailChannel {
    api_url: String,
    client: reqwest::Client,
}

impl EmailChannel {
    /// Build the channel for a Mailpit API base
    /// (e.g. `http://localhost:8025/api/v1`).
    #[must_use]
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Recipient address for an org-level notification.
    #[must_use]
    fn recipient(notification: &NotificationRow) -> String {
        match notification.recipient_org_id {
            Some(org) => format!("org-{org}@notify.acme-dev.local"),
            None => "unknown@notify.acme-dev.local".to_string(),
        }
    }
}

impl NotificationChannel for EmailChannel {
    fn deliver<'a>(
        &'a self,
        notification: &'a NotificationRow,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let (subject, text) = render(notification);
            let kind: NotificationType = match notification.notification_type.as_str() {
                "OFFER_RECEIVED" => NotificationType::OfferReceived,
                "COUNTER_OFFER_RECEIVED" => NotificationType::CounterOfferReceived,
                "OFFER_ACCEPTED" => NotificationType::OfferAccepted,
                _ => NotificationType::OfferRejected,
            };
            let _ = kind;

            let response = self
                .client
                .post(format!("{}/send", self.api_url))
                .json(&serde_json::json!({
                    "From": { "Email": "licensing@acme-dev.local", "Name": "ACME Licensing" },
                    "To": [ { "Email": Self::recipient(notification), "Name": "Licensing Team" } ],
                    "Subject": subject,
                    "Text": text,
                }))
                .send()
                .await?;
            anyhow::ensure!(
                response.status().is_success(),
                "mailpit rejected the message: {}",
                response.status()
            );
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(type_name: &str, org: Option<uuid::Uuid>) -> NotificationRow {
        NotificationRow {
            id: uuid::Uuid::now_v7(),
            recipient_user_id: None,
            recipient_org_id: org,
            notification_type: type_name.to_string(),
            payload: serde_json::json!({
                "license": {
                    "license_id": "01234567-89ab-cdef-0123-456789abcdef",
                    "movie_id": "movie", "song_id": "song",
                    "state": "COUNTER_OFFER", "license_fee_cents": 250_000,
                }
            }),
            source_event_id: uuid::Uuid::now_v7(),
            read_at: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn renders_actionable_subjects_and_bodies() {
        let org = uuid::Uuid::now_v7();
        let (subject, body) = render(&row("COUNTER_OFFER_RECEIVED", Some(org)));
        assert!(subject.starts_with("COUNTER_OFFER_RECEIVED: license 01234567"));
        assert!(body.contains("COUNTER_OFFER"));
        assert!(body.contains("$2500.00"));
        assert!(body.contains("01234567-89ab-cdef-0123-456789abcdef"));
    }

    #[test]
    fn recipients_are_org_derived() {
        let org = uuid::Uuid::now_v7();
        let notification = row("OFFER_RECEIVED", Some(org));
        assert_eq!(
            EmailChannel::recipient(&notification),
            format!("org-{org}@notify.acme-dev.local")
        );
    }
}
