//! Database rows and wire DTOs.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Row of the `notifications` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRow {
    /// Notification id (UUIDv7).
    pub id: Uuid,
    /// Direct recipient user, when targeted (future use).
    pub recipient_user_id: Option<Uuid>,
    /// Recipient organization (the counterparty of the acting side).
    pub recipient_org_id: Option<Uuid>,
    /// Notification type wire name.
    #[sqlx(rename = "type")]
    pub notification_type: String,
    /// Event payload (the license snapshot plus envelope fields).
    pub payload: serde_json::Value,
    /// Idempotency anchor: the license event that produced this row.
    pub source_event_id: Uuid,
    /// Read marker.
    pub read_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl NotificationRow {
    /// Public view.
    #[must_use]
    pub fn to_dto(&self) -> NotificationDto {
        NotificationDto {
            id: self.id,
            recipient_user_id: self.recipient_user_id,
            recipient_org_id: self.recipient_org_id,
            notification_type: self.notification_type.clone(),
            payload: self.payload.clone(),
            read_at: self.read_at,
            created_at: self.created_at,
        }
    }
}

/// Notification as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NotificationDto {
    /// Notification id.
    pub id: Uuid,
    /// Direct recipient user, when targeted.
    pub recipient_user_id: Option<Uuid>,
    /// Recipient organization.
    pub recipient_org_id: Option<Uuid>,
    /// Notification type.
    pub notification_type: String,
    /// Event payload.
    pub payload: serde_json::Value,
    /// Read marker.
    pub read_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}
