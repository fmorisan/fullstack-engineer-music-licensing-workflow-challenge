//! Kafka event contracts.
//!
//! These types define the JSON envelopes published to the [`crate::topics`]
//! topics via the transactional outbox (ADR-004) and consumed by the search
//! indexer and notification service. Their serialized shape is a cross-service
//! contract: the tests in this module pin the exact field names so accidental
//! renames fail here instead of downstream.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::license::LicenseState;

/// Kind of change to a song record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SongEventKind {
    /// A song entered the catalog.
    Created,
    /// Song metadata or media changed.
    Updated,
    /// A song left the catalog.
    Deleted,
}

/// The song record as carried on the wire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SongRecord {
    /// Catalog id of the song (UUIDv7).
    pub song_id: Uuid,
    /// Owning label organization id.
    pub label_id: Uuid,
    /// Song title.
    pub title: String,
    /// Song author.
    pub author: String,
    /// Total length in seconds.
    pub length_seconds: u32,
    /// S3 object key for box art, once uploaded.
    pub box_art_key: Option<String>,
    /// S3 object key for the (max 30s) audio preview, once uploaded.
    pub audio_preview_key: Option<String>,
}

/// Envelope published to the `song.events` topic.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SongEvent {
    /// Unique id of this event (UUIDv7); consumers deduplicate on it.
    pub event_id: Uuid,
    /// What happened to the song.
    pub kind: SongEventKind,
    /// When the change occurred.
    pub occurred_at: DateTime<Utc>,
    /// The song the event is about.
    pub song_id: Uuid,
    /// Last-known song record; `None` for [`SongEventKind::Deleted`].
    pub song: Option<SongRecord>,
}

/// Kind of change to a license.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LicenseEventKind {
    /// A license was created in [`LicenseState::Offer`].
    Created,
    /// A state transition was applied.
    StateChanged,
}

/// The license state as carried on the wire, post-transition.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LicenseSnapshot {
    /// License id (UUIDv7).
    pub license_id: Uuid,
    /// Movie the license is attached to.
    pub movie_id: Uuid,
    /// Scene number within the movie (ADR-009: per-movie integers).
    pub scene_number: i32,
    /// Song being licensed.
    pub song_id: Uuid,
    /// User that performed the change.
    pub actor_user_id: Uuid,
    /// Studio organization owning the movie.
    pub studio_id: Uuid,
    /// Label organization owning the song.
    pub label_id: Uuid,
    /// State before the change; `None` on creation.
    pub from_state: Option<LicenseState>,
    /// State after the change.
    pub state: LicenseState,
    /// Current fee in cents.
    pub license_fee_cents: i64,
    /// Playback window start within the scene, in seconds.
    pub start_time_seconds: u32,
    /// Playback window end within the scene, in seconds.
    pub end_time_seconds: u32,
    /// Id of the license log row this event was generated from; anchor for
    /// notification idempotency (ADR-006).
    pub license_log_id: Uuid,
}

/// Envelope published to the `license.events` topic.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LicenseEvent {
    /// Unique id of this event (UUIDv7); consumers deduplicate on it.
    pub event_id: Uuid,
    /// What happened to the license.
    pub kind: LicenseEventKind,
    /// When the change occurred.
    pub occurred_at: DateTime<Utc>,
    /// The license state after the change.
    pub license: LicenseSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song_record() -> SongRecord {
        SongRecord {
            song_id: Uuid::now_v7(),
            label_id: Uuid::now_v7(),
            title: "Nightcall".into(),
            author: "Kavinsky".into(),
            length_seconds: 252,
            box_art_key: Some("song-media/abc/box.jpg".into()),
            audio_preview_key: None,
        }
    }

    fn license_snapshot() -> LicenseSnapshot {
        LicenseSnapshot {
            license_id: Uuid::now_v7(),
            movie_id: Uuid::now_v7(),
            scene_number: 7,
            song_id: Uuid::now_v7(),
            actor_user_id: Uuid::now_v7(),
            studio_id: Uuid::now_v7(),
            label_id: Uuid::now_v7(),
            from_state: Some(LicenseState::Offer),
            state: LicenseState::CounterOffer,
            license_fee_cents: 125_000,
            start_time_seconds: 10,
            end_time_seconds: 40,
            license_log_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn song_event_round_trips() {
        let event = SongEvent {
            event_id: Uuid::now_v7(),
            kind: SongEventKind::Created,
            occurred_at: Utc::now(),
            song_id: song_record().song_id,
            song: Some(song_record()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: SongEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn song_event_json_contract_is_pinned() {
        let song = song_record();
        let event = SongEvent {
            event_id: Uuid::now_v7(),
            kind: SongEventKind::Deleted,
            occurred_at: Utc::now(),
            song_id: song.song_id,
            song: None,
        };
        let v = serde_json::to_value(&event).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["event_id", "kind", "occurred_at", "song", "song_id"]);
        assert_eq!(v["kind"], "DELETED");
        assert!(v["song"].is_null());

        let sv = serde_json::to_value(&song).unwrap();
        let mut skeys: Vec<&str> = sv.as_object().unwrap().keys().map(String::as_str).collect();
        skeys.sort_unstable();
        let mut expected = [
            "song_id",
            "label_id",
            "title",
            "author",
            "length_seconds",
            "box_art_key",
            "audio_preview_key",
        ];
        expected.sort_unstable();
        assert_eq!(skeys, expected);
    }

    #[test]
    fn license_event_round_trips() {
        let event = LicenseEvent {
            event_id: Uuid::now_v7(),
            kind: LicenseEventKind::StateChanged,
            occurred_at: Utc::now(),
            license: license_snapshot(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: LicenseEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn license_event_json_contract_is_pinned() {
        let event = LicenseEvent {
            event_id: Uuid::now_v7(),
            kind: LicenseEventKind::Created,
            occurred_at: Utc::now(),
            license: license_snapshot(),
        };
        let v = serde_json::to_value(&event).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["event_id", "kind", "license", "occurred_at"]);
        assert_eq!(v["kind"], "CREATED");

        let ls = &v["license"];
        assert_eq!(ls["state"], "COUNTER_OFFER");
        assert_eq!(ls["from_state"], "OFFER");
        assert_eq!(ls["license_fee_cents"], 125_000);
        assert_eq!(ls["scene_number"], 7);
    }

    #[test]
    fn song_event_kinds_serialize_as_expected() {
        assert_eq!(
            serde_json::to_string(&SongEventKind::Created).unwrap(),
            "\"CREATED\""
        );
        assert_eq!(
            serde_json::to_string(&SongEventKind::Updated).unwrap(),
            "\"UPDATED\""
        );
        assert_eq!(
            serde_json::to_string(&SongEventKind::Deleted).unwrap(),
            "\"DELETED\""
        );
        assert_eq!(
            serde_json::to_string(&LicenseEventKind::StateChanged).unwrap(),
            "\"STATE_CHANGED\""
        );
    }
}
