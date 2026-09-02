//! Database rows and wire DTOs.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Row of the `songs` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SongRow {
    /// Song id (UUIDv7).
    pub id: Uuid,
    /// Owning label organization.
    pub label_id: Uuid,
    /// Song title.
    pub title: String,
    /// Song author.
    pub author: String,
    /// Total length in seconds.
    pub length_seconds: i32,
    /// Box art object key, once uploaded.
    pub box_art_key: Option<String>,
    /// Audio preview object key, once uploaded.
    pub audio_preview_key: Option<String>,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Row last-update time.
    pub updated_at: DateTime<Utc>,
}

impl SongRow {
    /// The wire record carried on Kafka events (licensing-core contract).
    #[must_use]
    pub fn to_record(&self) -> licensing_core::SongRecord {
        licensing_core::SongRecord {
            song_id: self.id,
            label_id: self.label_id,
            title: self.title.clone(),
            author: self.author.clone(),
            length_seconds: u32::try_from(self.length_seconds).unwrap_or(0),
            box_art_key: self.box_art_key.clone(),
            audio_preview_key: self.audio_preview_key.clone(),
        }
    }

    /// Public API view (same shape as the record, plus timestamps).
    #[must_use]
    pub fn to_dto(&self) -> SongDto {
        SongDto {
            id: self.id,
            label_id: self.label_id,
            title: self.title.clone(),
            author: self.author.clone(),
            length_seconds: self.length_seconds,
            box_art_key: self.box_art_key.clone(),
            audio_preview_key: self.audio_preview_key.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Song as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SongDto {
    /// Song id.
    pub id: Uuid,
    /// Owning label organization.
    pub label_id: Uuid,
    /// Song title.
    pub title: String,
    /// Song author.
    pub author: String,
    /// Total length in seconds.
    pub length_seconds: i32,
    /// Box art object key, when uploaded.
    pub box_art_key: Option<String>,
    /// Audio preview object key, when uploaded.
    pub audio_preview_key: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last-update time.
    pub updated_at: DateTime<Utc>,
}
