//! Pre-signed media uploads: box art and audio previews (ADR-009).

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Previews are capped at 30 seconds (00-design.md).
pub const MAX_PREVIEW_SECONDS: i32 = 30;

/// Body of the presign endpoints.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Content type the client will upload; signed into the URL when set.
    pub content_type: Option<String>,
    /// Declared duration of the audio preview, in seconds.
    pub duration_seconds: Option<i32>,
}

async fn owned_song_or_404(
    state: &AppState,
    user: &AuthenticatedUser,
    song_id: Uuid,
) -> ApiResult<()> {
    let label_id = user.org_id.ok_or(ApiError::Forbidden)?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM songs WHERE id = $1 AND label_id = $2)"#,
        song_id,
        label_id,
    )
    .fetch_one(state.pool())
    .await?
    .unwrap_or(false);
    if exists {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /songs/:id/box_art` — mint a pre-signed upload URL for box art.
///
/// # Errors
///
/// `404` for missing/foreign songs, `403` for non-label users.
pub async fn box_art(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(song_id): Path<Uuid>,
    Json(req): Json<PresignRequest>,
) -> ApiResult<Json<platform::PresignedUpload>> {
    user.ensure_role(licensing_core::Role::Label)
        .map_err(|_| ApiError::Forbidden)?;
    owned_song_or_404(&state, &user, song_id).await?;

    let key = format!(
        "songs/{song_id}/box-art/{}",
        licensing_core::new_id().simple()
    );
    let upload = state
        .media()
        .presign_put(&key, req.content_type.as_deref())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(upload))
}

/// `PUT /songs/:id/audio_preview` — mint a pre-signed upload URL for the
/// audio preview. The declared duration must not exceed 30 seconds
/// (00-design.md); actual encoded duration is the client's responsibility.
///
/// # Errors
///
/// `404` for missing/foreign songs, `403` for non-label users, `422` when
/// the declared duration exceeds the cap.
pub async fn audio_preview(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(song_id): Path<Uuid>,
    Json(req): Json<PresignRequest>,
) -> ApiResult<Json<platform::PresignedUpload>> {
    user.ensure_role(licensing_core::Role::Label)
        .map_err(|_| ApiError::Forbidden)?;
    owned_song_or_404(&state, &user, song_id).await?;

    let duration = req.duration_seconds.unwrap_or(MAX_PREVIEW_SECONDS);
    if !(0..=MAX_PREVIEW_SECONDS).contains(&duration) {
        return Err(ApiError::Validation(format!(
            "audio previews are capped at {MAX_PREVIEW_SECONDS} seconds"
        )));
    }

    let key = format!(
        "songs/{song_id}/preview/{}",
        licensing_core::new_id().simple()
    );
    let upload = state
        .media()
        .presign_put(&key, req.content_type.as_deref())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(upload))
}
