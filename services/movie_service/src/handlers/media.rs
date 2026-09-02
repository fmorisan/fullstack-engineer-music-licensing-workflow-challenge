//! Pre-signed media uploads: posters and scene captures (ADR-009).

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::media::PresignedUpload;
use crate::state::AppState;

/// Body of the presign endpoints.
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    /// Content type the client will upload; signed into the URL when set, so
    /// the uploaded bytes must match.
    pub content_type: Option<String>,
}

async fn owned_movie_or_404(
    state: &AppState,
    user: &AuthenticatedUser,
    movie_id: Uuid,
) -> ApiResult<()> {
    let studio_id = user.org_id.ok_or(ApiError::Forbidden)?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM movies WHERE id = $1 AND studio_id = $2)"#,
        movie_id,
        studio_id,
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

/// `PUT /movies/:id/poster` — mint a pre-signed upload URL for the poster.
///
/// # Errors
///
/// `404` for missing/foreign movies, `403` for non-studio users.
pub async fn poster(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(movie_id): Path<Uuid>,
    Json(req): Json<PresignRequest>,
) -> ApiResult<Json<PresignedUpload>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    owned_movie_or_404(&state, &user, movie_id).await?;

    let key = format!(
        "movies/{movie_id}/poster/{}",
        licensing_core::new_id().simple()
    );
    let upload = state
        .media()
        .presign_put(&key, req.content_type.as_deref())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(upload))
}

/// `PUT /movies/:id/scenes/:n/capture` — mint a pre-signed upload URL for a
/// scene capture image.
///
/// # Errors
///
/// `404` for missing movies or scenes, `403` for non-studio users.
pub async fn capture(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((movie_id, scene_number)): Path<(Uuid, i32)>,
    Json(req): Json<PresignRequest>,
) -> ApiResult<Json<PresignedUpload>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    owned_movie_or_404(&state, &user, movie_id).await?;

    let scene_exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM scenes WHERE movie_id = $1 AND scene_number = $2)"#,
        movie_id,
        scene_number,
    )
    .fetch_one(state.pool())
    .await?
    .unwrap_or(false);
    if !scene_exists {
        return Err(ApiError::NotFound);
    }

    let key = format!(
        "movies/{movie_id}/scenes/{scene_number}/capture/{}",
        licensing_core::new_id().simple()
    );
    let upload = state
        .media()
        .presign_put(&key, req.content_type.as_deref())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(upload))
}
