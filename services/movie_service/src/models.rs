//! Database rows and wire DTOs.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Row of the `movies` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MovieRow {
    /// Movie id (UUIDv7).
    pub id: Uuid,
    /// Owning studio organization.
    pub studio_id: Uuid,
    /// Movie title.
    pub title: String,
    /// Short description.
    pub description: String,
    /// S3 object key for the poster, once uploaded.
    pub poster_key: Option<String>,
    /// Row creation time.
    pub created_at: DateTime<Utc>,
    /// Row last-update time.
    pub updated_at: DateTime<Utc>,
}

/// Movie as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MovieDto {
    /// Movie id.
    pub id: Uuid,
    /// Owning studio organization.
    pub studio_id: Uuid,
    /// Movie title.
    pub title: String,
    /// Short description.
    pub description: String,
    /// Poster object key, when uploaded.
    pub poster_key: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last-update time.
    pub updated_at: DateTime<Utc>,
}

impl From<MovieRow> for MovieDto {
    fn from(row: MovieRow) -> Self {
        Self {
            id: row.id,
            studio_id: row.studio_id,
            title: row.title,
            description: row.description,
            poster_key: row.poster_key,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Row of the `scenes` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SceneRow {
    /// Parent movie.
    pub movie_id: Uuid,
    /// Scene number within the movie (ADR-009).
    pub scene_number: i32,
    /// Scene duration in seconds.
    pub screen_time_seconds: i32,
    /// Start of the scene, seconds from film start.
    pub start_time_seconds: i32,
    /// End of the scene (exclusive), seconds from film start.
    pub end_time_seconds: i32,
    /// Short description.
    pub description: String,
    /// Capture image object key, when uploaded.
    pub capture_key: Option<String>,
}

/// Scene as exposed over the wire.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SceneDto {
    /// Parent movie id.
    pub movie_id: Uuid,
    /// Scene number within the movie.
    pub scene_number: i32,
    /// Scene duration in seconds.
    pub screen_time_seconds: i32,
    /// Start of the scene, seconds from film start.
    pub start_time_seconds: i32,
    /// End of the scene (exclusive), seconds from film start.
    pub end_time_seconds: i32,
    /// Short description.
    pub description: String,
    /// Capture image object key, when uploaded.
    pub capture_key: Option<String>,
}

impl From<SceneRow> for SceneDto {
    fn from(row: SceneRow) -> Self {
        Self {
            movie_id: row.movie_id,
            scene_number: row.scene_number,
            screen_time_seconds: row.screen_time_seconds,
            start_time_seconds: row.start_time_seconds,
            end_time_seconds: row.end_time_seconds,
            description: row.description,
            capture_key: row.capture_key,
        }
    }
}
