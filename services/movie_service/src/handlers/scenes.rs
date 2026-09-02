//! Scene creation and updates, guarded by the database EXCLUDE constraint.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{MovieRow, SceneDto, SceneRow};
use crate::state::AppState;

/// Body of `PUT /movies/:id/scenes` and `PUT /movies/:id/scenes/:n`.
#[derive(Debug, Deserialize)]
pub struct SceneRequest {
    /// Scene duration in seconds.
    pub screen_time_seconds: i32,
    /// Start of the scene, seconds from film start.
    pub start_time_seconds: i32,
    /// End of the scene (exclusive), seconds from film start.
    pub end_time_seconds: i32,
    /// Short description.
    pub description: String,
    /// Capture image object key, set after a successful pre-signed upload.
    pub capture_key: Option<String>,
}

fn validate(req: &SceneRequest) -> ApiResult<()> {
    if req.screen_time_seconds <= 0 {
        return Err(ApiError::Validation(
            "screen_time_seconds must be positive".into(),
        ));
    }
    if req.start_time_seconds < 0 {
        return Err(ApiError::Validation(
            "start_time_seconds must not be negative".into(),
        ));
    }
    if req.end_time_seconds <= req.start_time_seconds {
        return Err(ApiError::Validation(
            "end_time_seconds must be greater than start_time_seconds".into(),
        ));
    }
    if req.description.len() > 2000 {
        return Err(ApiError::Validation(
            "description must be at most 2000 characters".into(),
        ));
    }
    Ok(())
}

/// Map an exclusion or check violation from the scenes table to 409.
fn map_scene_violation(err: sqlx::Error) -> ApiError {
    let is_scene_violation = err.as_database_error().is_some_and(|db| {
        // Postgres exclusion violations surface as SQLSTATE 23P01.
        db.code().is_some_and(|code| code == "23P01")
            || db.is_check_violation()
            || db.constraint().is_some_and(|c| c.contains("scenes"))
    });
    if is_scene_violation {
        ApiError::Conflict("scene time range overlaps an existing scene".into())
    } else {
        ApiError::Internal(err.into())
    }
}

async fn owned_movie_or_404(
    state: &AppState,
    user: &AuthenticatedUser,
    movie_id: Uuid,
) -> ApiResult<MovieRow> {
    let studio_id = user.org_id.ok_or(ApiError::Forbidden)?;
    sqlx::query_as!(
        MovieRow,
        "SELECT id, studio_id, title, description, poster_key, created_at, updated_at
         FROM movies WHERE id = $1 AND studio_id = $2",
        movie_id,
        studio_id,
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or(ApiError::NotFound)
}

/// `PUT /movies/:id/scenes` — append a scene with the next scene number.
///
/// Overlap is rejected by the database EXCLUDE constraint and surfaced as
/// `409`; adjacency is allowed.
///
/// # Errors
///
/// `404` for missing/foreign movies, `422` on validation, `409` on overlap.
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(movie_id): Path<Uuid>,
    Json(req): Json<SceneRequest>,
) -> ApiResult<(StatusCode, Json<SceneDto>)> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;
    owned_movie_or_404(&state, &user, movie_id).await?;

    // Auto-number inside a transaction; retry on the unlikely race for the
    // next slot.
    for _ in 0..3 {
        let mut tx = state.pool().begin().await?;
        let scene_number: Option<i32> = sqlx::query_scalar!(
            "SELECT COALESCE(MAX(scene_number), 0) + 1 FROM scenes WHERE movie_id = $1",
            movie_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let inserted = sqlx::query_as!(
            SceneRow,
            "INSERT INTO scenes
                (movie_id, scene_number, screen_time_seconds, start_time_seconds,
                 end_time_seconds, description, capture_key)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING movie_id, scene_number, screen_time_seconds, start_time_seconds,
                       end_time_seconds, description, capture_key",
            movie_id,
            scene_number,
            req.screen_time_seconds,
            req.start_time_seconds,
            req.end_time_seconds,
            req.description.trim(),
            req.capture_key,
        )
        .fetch_one(&mut *tx)
        .await;

        match inserted {
            Ok(row) => {
                tx.commit().await?;
                return Ok((StatusCode::CREATED, Json(row.into())));
            }
            Err(err) => {
                // Unique violation on (movie_id, scene_number): a concurrent
                // writer took our slot; retry. Everything else is real.
                let is_number_race = err
                    .as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation);
                if is_number_race {
                    continue;
                }
                return Err(map_scene_violation(err));
            }
        }
    }
    Err(ApiError::Conflict("scene number race; retry".into()))
}

/// `PUT /movies/:id/scenes/:n` — update a scene in place.
///
/// # Errors
///
/// `404` for missing scenes, `422` on validation, `409` on overlap.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path((movie_id, scene_number)): Path<(Uuid, i32)>,
    Json(req): Json<SceneRequest>,
) -> ApiResult<Json<SceneDto>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;
    owned_movie_or_404(&state, &user, movie_id).await?;

    let row = sqlx::query_as!(
        SceneRow,
        "UPDATE scenes
         SET screen_time_seconds = $3, start_time_seconds = $4, end_time_seconds = $5,
             description = $6, capture_key = COALESCE($7, capture_key)
         WHERE movie_id = $1 AND scene_number = $2
         RETURNING movie_id, scene_number, screen_time_seconds, start_time_seconds,
                   end_time_seconds, description, capture_key",
        movie_id,
        scene_number,
        req.screen_time_seconds,
        req.start_time_seconds,
        req.end_time_seconds,
        req.description.trim(),
        req.capture_key,
    )
    .fetch_optional(state.pool())
    .await
    .map_err(map_scene_violation)?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(row.into()))
}
