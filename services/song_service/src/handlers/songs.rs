//! Song catalog CRUD, scoped to the caller's label organization.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{SongDto, SongRow};
use crate::state::AppState;

/// Body of `POST /songs` and `PUT /songs/:id`.
#[derive(Debug, Deserialize)]
pub struct SongRequest {
    /// Song title.
    pub title: String,
    /// Song author.
    pub author: String,
    /// Total length in seconds.
    pub length_seconds: i32,
    /// Box art object key, set after a successful pre-signed upload.
    pub box_art_key: Option<String>,
    /// Audio preview object key, set after a successful pre-signed upload.
    pub audio_preview_key: Option<String>,
}

fn validate(req: &SongRequest) -> ApiResult<()> {
    let title = req.title.trim();
    let author = req.author.trim();
    if title.is_empty() {
        return Err(ApiError::Validation("title must not be empty".into()));
    }
    if title.len() > 200 || author.len() > 200 {
        return Err(ApiError::Validation(
            "title and author must be at most 200 characters".into(),
        ));
    }
    if author.is_empty() {
        return Err(ApiError::Validation("author must not be empty".into()));
    }
    if req.length_seconds <= 0 {
        return Err(ApiError::Validation(
            "length_seconds must be positive".into(),
        ));
    }
    Ok(())
}

fn label_org(user: &AuthenticatedUser) -> ApiResult<Uuid> {
    user.org_id.ok_or(ApiError::Forbidden)
}

/// Load a song owned by the caller's label; others' songs are 404s.
async fn owned_song(
    state: &AppState,
    user: &AuthenticatedUser,
    song_id: Uuid,
) -> ApiResult<SongRow> {
    sqlx::query_as!(
        SongRow,
        "SELECT id, label_id, title, author, length_seconds, box_art_key,
                audio_preview_key, created_at, updated_at
         FROM songs WHERE id = $1 AND label_id = $2",
        song_id,
        label_org(user)?,
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or(ApiError::NotFound)
}

/// `GET /songs` — list the caller's label's catalog.
///
/// # Errors
///
/// `403` for non-label users.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<Json<Vec<SongDto>>> {
    user.ensure_role(licensing_core::Role::Label)
        .map_err(|_| ApiError::Forbidden)?;
    let rows = sqlx::query_as!(
        SongRow,
        "SELECT id, label_id, title, author, length_seconds, box_art_key,
                audio_preview_key, created_at, updated_at
         FROM songs WHERE label_id = $1 ORDER BY created_at DESC",
        label_org(&user)?,
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(rows.iter().map(SongRow::to_dto).collect()))
}

/// `POST /songs` — add a song to the caller's catalog.
///
/// # Errors
///
/// `403` for non-label users, `422` on validation failures.
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<SongRequest>,
) -> ApiResult<(StatusCode, Json<SongDto>)> {
    user.ensure_role(licensing_core::Role::Label)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;
    let label_id = label_org(&user)?;

    let row = sqlx::query_as!(
        SongRow,
        "INSERT INTO songs (id, label_id, title, author, length_seconds, box_art_key, audio_preview_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, label_id, title, author, length_seconds, box_art_key,
                   audio_preview_key, created_at, updated_at",
        licensing_core::new_id(),
        label_id,
        req.title.trim(),
        req.author.trim(),
        req.length_seconds,
        req.box_art_key,
        req.audio_preview_key,
    )
    .fetch_one(state.pool())
    .await?;

    Ok((StatusCode::CREATED, Json(row.to_dto())))
}

/// `PUT /songs/:id` — update catalog metadata (and record uploaded keys).
///
/// # Errors
///
/// `404` when not owned, `422` on validation failures.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(song_id): Path<Uuid>,
    Json(req): Json<SongRequest>,
) -> ApiResult<Json<SongDto>> {
    user.ensure_role(licensing_core::Role::Label)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;
    let label_id = label_org(&user)?;
    owned_song(&state, &user, song_id).await?;

    let row = sqlx::query_as!(
        SongRow,
        "UPDATE songs
         SET title = $3, author = $4, length_seconds = $5,
             box_art_key = COALESCE($6, box_art_key),
             audio_preview_key = COALESCE($7, audio_preview_key),
             updated_at = now()
         WHERE id = $1 AND label_id = $2
         RETURNING id, label_id, title, author, length_seconds, box_art_key,
                   audio_preview_key, created_at, updated_at",
        song_id,
        label_id,
        req.title.trim(),
        req.author.trim(),
        req.length_seconds,
        req.box_art_key,
        req.audio_preview_key,
    )
    .fetch_one(state.pool())
    .await?;

    Ok(Json(row.to_dto()))
}
