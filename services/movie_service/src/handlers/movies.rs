//! Movie CRUD, scoped to the caller's studio organization.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{MovieDto, MovieRow};
use crate::state::AppState;

/// Body of `POST /movies` and `PUT /movies/:id`.
#[derive(Debug, Deserialize)]
pub struct MovieRequest {
    /// Movie title.
    pub title: String,
    /// Short description.
    pub description: String,
    /// Poster object key, set after a successful pre-signed upload.
    pub poster_key: Option<String>,
}

fn validate(req: &MovieRequest) -> ApiResult<()> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err(ApiError::Validation("title must not be empty".into()));
    }
    if title.len() > 200 {
        return Err(ApiError::Validation(
            "title must be at most 200 characters".into(),
        ));
    }
    if req.description.len() > 2000 {
        return Err(ApiError::Validation(
            "description must be at most 2000 characters".into(),
        ));
    }
    Ok(())
}

/// The caller's studio organization; studio users always carry one.
fn studio_org(user: &AuthenticatedUser) -> ApiResult<Uuid> {
    user.org_id.ok_or(ApiError::Forbidden)
}

/// Load a movie owned by the caller's studio; other studios' movies are 404s
/// (no existence leak).
async fn owned_movie(
    state: &AppState,
    user: &AuthenticatedUser,
    movie_id: Uuid,
) -> ApiResult<MovieRow> {
    let movie = sqlx::query_as!(
        MovieRow,
        "SELECT id, studio_id, title, description, poster_key, created_at, updated_at
         FROM movies WHERE id = $1 AND studio_id = $2",
        movie_id,
        studio_org(user)?,
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(movie)
}

/// `GET /movies` — list the caller's studio's movies.
///
/// # Errors
///
/// `403` for non-studio users.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<Json<Vec<MovieDto>>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    let rows = sqlx::query_as!(
        MovieRow,
        "SELECT id, studio_id, title, description, poster_key, created_at, updated_at
         FROM movies WHERE studio_id = $1 ORDER BY created_at DESC",
        studio_org(&user)?,
    )
    .fetch_all(state.pool())
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// `POST /movies` — create a movie for the caller's studio.
///
/// # Errors
///
/// `403` for non-studio users, `422` on validation failures.
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<MovieRequest>,
) -> ApiResult<(StatusCode, Json<MovieDto>)> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;

    let row = sqlx::query_as!(
        MovieRow,
        "INSERT INTO movies (id, studio_id, title, description, poster_key)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, studio_id, title, description, poster_key, created_at, updated_at",
        licensing_core::new_id(),
        studio_org(&user)?,
        req.title.trim(),
        req.description.trim(),
        req.poster_key,
    )
    .fetch_one(state.pool())
    .await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// `GET /movies/:id` — movie detail.
///
/// # Errors
///
/// `404` when the movie does not exist or belongs to another studio.
pub async fn detail(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(movie_id): Path<Uuid>,
) -> ApiResult<Json<MovieDto>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    let movie = owned_movie(&state, &user, movie_id).await?;
    Ok(Json(movie.into()))
}

/// `PUT /movies/:id` — update title/description (and record an uploaded
/// poster key).
///
/// # Errors
///
/// `404` when not owned, `422` on validation failures.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(movie_id): Path<Uuid>,
    Json(req): Json<MovieRequest>,
) -> ApiResult<Json<MovieDto>> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    validate(&req)?;
    owned_movie(&state, &user, movie_id).await?;

    let row = sqlx::query_as!(
        MovieRow,
        "UPDATE movies
         SET title = $3, description = $4, poster_key = COALESCE($5, poster_key),
             updated_at = now()
         WHERE id = $1 AND studio_id = $2
         RETURNING id, studio_id, title, description, poster_key, created_at, updated_at",
        movie_id,
        studio_org(&user)?,
        req.title.trim(),
        req.description.trim(),
        req.poster_key,
    )
    .fetch_one(state.pool())
    .await?;
    Ok(Json(row.into()))
}
