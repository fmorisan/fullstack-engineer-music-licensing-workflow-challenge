//! Database rows and wire DTOs.

use chrono::{DateTime, Utc};
use licensing_core::LicenseState;
use uuid::Uuid;

/// Row of the `licenses` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LicenseRow {
    /// License id (UUIDv7).
    pub id: Uuid,
    /// Movie the license attaches to.
    pub movie_id: Uuid,
    /// Scene number within the movie.
    pub scene_number: i32,
    /// Song being licensed.
    pub song_id: Uuid,
    /// Studio organization.
    pub studio_id: Uuid,
    /// Label organization.
    pub label_id: Uuid,
    /// Studio user who created the license.
    pub studio_user_id: Uuid,
    /// State wire name (constrained).
    pub state: String,
    /// Current fee in cents.
    pub license_fee_cents: i64,
    /// Playback start within the scene, seconds.
    pub start_time_seconds: i32,
    /// Playback end within the scene, seconds (exclusive).
    pub end_time_seconds: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last transition time.
    pub updated_at: DateTime<Utc>,
}

impl LicenseRow {
    /// Parse the state into the domain enum.
    ///
    /// # Panics
    ///
    /// Never for committed rows (CHECK constraint).
    #[must_use]
    pub fn domain_state(&self) -> LicenseState {
        self.state
            .parse()
            .expect("state column constrained to wire names")
    }

    /// Public view.
    #[must_use]
    pub fn to_dto(&self) -> LicenseDto {
        LicenseDto {
            id: self.id,
            movie_id: self.movie_id,
            scene_number: self.scene_number,
            song_id: self.song_id,
            studio_id: self.studio_id,
            label_id: self.label_id,
            state: self.domain_state(),
            license_fee_cents: self.license_fee_cents,
            start_time_seconds: self.start_time_seconds,
            end_time_seconds: self.end_time_seconds,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// License as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LicenseDto {
    /// License id.
    pub id: Uuid,
    /// Movie id.
    pub movie_id: Uuid,
    /// Scene number.
    pub scene_number: i32,
    /// Song id.
    pub song_id: Uuid,
    /// Studio organization.
    pub studio_id: Uuid,
    /// Label organization.
    pub label_id: Uuid,
    /// Lifecycle state.
    pub state: LicenseState,
    /// Current fee in cents.
    pub license_fee_cents: i64,
    /// Playback start, seconds within the scene.
    pub start_time_seconds: i32,
    /// Playback end, seconds within the scene (exclusive).
    pub end_time_seconds: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last transition time.
    pub updated_at: DateTime<Utc>,
}

/// Row of the `license_log` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LicenseLogRow {
    /// Entry id.
    pub id: Uuid,
    /// License the entry belongs to.
    pub license_id: Uuid,
    /// Monotonic sequence within the license (1 = creation).
    pub sequence: i32,
    /// Previous state; `None` on creation.
    pub from_state: Option<String>,
    /// State after the transition.
    pub to_state: String,
    /// Action applied (`CREATE`, or a [`licensing_core::LicenseAction`] name).
    pub action: String,
    /// Acting user.
    pub actor_user_id: Uuid,
    /// Fee in effect after the transition.
    pub license_fee_cents: i64,
    /// Transition time.
    pub created_at: DateTime<Utc>,
}

impl LicenseLogRow {
    /// Public view.
    #[must_use]
    pub fn to_dto(&self) -> LicenseLogDto {
        LicenseLogDto {
            sequence: self.sequence,
            from_state: self.from_state.clone(),
            to_state: self.to_state.clone(),
            action: self.action.clone(),
            actor_user_id: self.actor_user_id,
            license_fee_cents: self.license_fee_cents,
            created_at: self.created_at,
        }
    }
}

/// Log entry as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LicenseLogDto {
    /// Sequence within the license.
    pub sequence: i32,
    /// Previous state.
    pub from_state: Option<String>,
    /// State after.
    pub to_state: String,
    /// Action applied.
    pub action: String,
    /// Acting user.
    pub actor_user_id: Uuid,
    /// Fee after the transition.
    pub license_fee_cents: i64,
    /// Transition time.
    pub created_at: DateTime<Utc>,
}

/// License detail including its lifecycle log.
#[derive(Debug, serde::Serialize)]
pub struct LicenseDetailDto {
    /// The license.
    #[serde(flatten)]
    pub license: LicenseDto,
    /// Lifecycle log, ordered by sequence.
    pub log: Vec<LicenseLogDto>,
}
