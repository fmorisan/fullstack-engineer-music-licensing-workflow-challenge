//! License creation (studio offers) and reads.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use platform::AuthenticatedUser;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{LicenseDetailDto, LicenseDto, LicenseLogRow, LicenseRow};
use crate::state::AppState;

/// Body of `POST /licenses`.
#[derive(Debug, Deserialize)]
pub struct CreateLicenseRequest {
    /// Movie to license within.
    pub movie_id: Uuid,
    /// Scene number within the movie.
    pub scene_number: i32,
    /// Song to license.
    pub song_id: Uuid,
    /// Playback start within the scene, seconds.
    pub start_time_seconds: i32,
    /// Playback end within the scene, seconds (exclusive).
    pub end_time_seconds: i32,
    /// Offered fee in cents.
    pub license_fee_cents: i64,
}

/// Query of `GET /licenses`.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// Filter by movie (required for studio callers).
    pub movie_id: Option<Uuid>,
    /// Optional scene filter within the movie.
    pub scene_number: Option<i32>,
    /// Optional song filter (label side).
    pub song_id: Option<Uuid>,
}

/// `POST /licenses` — a studio offers to license a song for a scene.
///
/// Validates the movie/scene against movie_service and the song against
/// song_service (caller's JWT propagated; ADR-001), then creates the license
/// in state `OFFER` with the first license-log entry — atomically.
///
/// # Errors
///
/// `403` for non-studio users; `404` for missing/foreign movies, scenes, or
/// songs; `422` for invalid times or fees.
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateLicenseRequest>,
) -> ApiResult<(StatusCode, Json<LicenseDto>)> {
    user.ensure_role(licensing_core::Role::Studio)
        .map_err(|_| ApiError::Forbidden)?;
    let studio_id = user.org_id.ok_or(ApiError::Forbidden)?;

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
    if req.license_fee_cents < 0 {
        return Err(ApiError::Validation(
            "license_fee_cents must not be negative".into(),
        ));
    }

    // Upstream validation, with the caller's identity.
    let movie = state
        .upstream()
        .movie(&user, req.movie_id)
        .await?
        .ok_or_else(|| ApiError::NotFound)?;
    if movie.studio_id != studio_id {
        // Not the caller's movie — do not leak existence.
        return Err(ApiError::NotFound);
    }
    let Some((_, screen_time)) = movie
        .scenes
        .iter()
        .find(|(number, _)| *number == req.scene_number)
    else {
        return Err(ApiError::NotFound);
    };
    if req.end_time_seconds > *screen_time {
        return Err(ApiError::Validation(format!(
            "playback window must fit within the scene's {screen_time}s"
        )));
    }

    let song = state
        .upstream()
        .song(&user, req.song_id)
        .await?
        .ok_or_else(|| ApiError::NotFound)?;

    let license_id = licensing_core::new_id();
    let mut tx = state.pool().begin().await?;
    let row = sqlx::query_as!(
        LicenseRow,
        "INSERT INTO licenses
            (id, movie_id, scene_number, song_id, studio_id, label_id,
             studio_user_id, state, license_fee_cents, start_time_seconds, end_time_seconds)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id, movie_id, scene_number, song_id, studio_id, label_id,
                   studio_user_id, state, license_fee_cents, start_time_seconds,
                   end_time_seconds, created_at, updated_at",
        license_id,
        req.movie_id,
        req.scene_number,
        req.song_id,
        studio_id,
        song.label_id,
        user.user_id,
        licensing_core::LicenseState::INITIAL.as_str(),
        req.license_fee_cents,
        req.start_time_seconds,
        req.end_time_seconds,
    )
    .fetch_one(&mut *tx)
    .await?;

    let log_id = licensing_core::new_id();
    sqlx::query!(
        "INSERT INTO license_log
            (id, license_id, sequence, from_state, to_state, action, actor_user_id, license_fee_cents)
         VALUES ($1, $2, 1, NULL, $3, 'CREATE', $4, $5)",
        log_id,
        license_id,
        licensing_core::LicenseState::INITIAL.as_str(),
        user.user_id,
        req.license_fee_cents,
    )
    .execute(&mut *tx)
    .await?;

    let snapshot = crate::events::snapshot(&row, None, &user, log_id);
    crate::events::enqueue(
        &mut *tx,
        licensing_core::LicenseEventKind::Created,
        &snapshot,
    )
    .await?;

    tx.commit().await?;
    crate::events::publish_live(&state, &snapshot).await;
    Ok((StatusCode::CREATED, Json(row.to_dto())))
}

/// `GET /licenses` — studios see one movie's licenses; labels see incoming
/// licenses for their songs.
///
/// # Errors
///
/// `403` for roles other than STUDIO/LABEL; `422` when a studio omits
/// `movie_id`.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Vec<LicenseDto>>> {
    let filter_org = user.org_id.ok_or(ApiError::Forbidden)?;

    let rows = match user.role {
        licensing_core::Role::Studio => {
            let movie_id = params.movie_id.ok_or_else(|| {
                ApiError::Validation("movie_id is required for studio callers".into())
            })?;
            if let Some(scene_number) = params.scene_number {
                sqlx::query_as!(
                    LicenseRow,
                    "SELECT id, movie_id, scene_number, song_id, studio_id, label_id,
                            studio_user_id, state, license_fee_cents, start_time_seconds,
                            end_time_seconds, created_at, updated_at
                     FROM licenses
                     WHERE studio_id = $1 AND movie_id = $2 AND scene_number = $3
                     ORDER BY created_at DESC",
                    filter_org,
                    movie_id,
                    scene_number,
                )
                .fetch_all(state.pool())
                .await?
            } else {
                sqlx::query_as!(
                    LicenseRow,
                    "SELECT id, movie_id, scene_number, song_id, studio_id, label_id,
                            studio_user_id, state, license_fee_cents, start_time_seconds,
                            end_time_seconds, created_at, updated_at
                     FROM licenses
                     WHERE studio_id = $1 AND movie_id = $2
                     ORDER BY created_at DESC",
                    filter_org,
                    movie_id,
                )
                .fetch_all(state.pool())
                .await?
            }
        }
        licensing_core::Role::Label => {
            if let Some(song_id) = params.song_id {
                sqlx::query_as!(
                    LicenseRow,
                    "SELECT id, movie_id, scene_number, song_id, studio_id, label_id,
                            studio_user_id, state, license_fee_cents, start_time_seconds,
                            end_time_seconds, created_at, updated_at
                     FROM licenses
                     WHERE label_id = $1 AND song_id = $2
                     ORDER BY created_at DESC",
                    filter_org,
                    song_id,
                )
                .fetch_all(state.pool())
                .await?
            } else {
                sqlx::query_as!(
                    LicenseRow,
                    "SELECT id, movie_id, scene_number, song_id, studio_id, label_id,
                            studio_user_id, state, license_fee_cents, start_time_seconds,
                            end_time_seconds, created_at, updated_at
                     FROM licenses
                     WHERE label_id = $1
                     ORDER BY created_at DESC",
                    filter_org,
                )
                .fetch_all(state.pool())
                .await?
            }
        }
        licensing_core::Role::Admin => return Err(ApiError::Forbidden),
    };

    Ok(Json(rows.iter().map(LicenseRow::to_dto).collect()))
}

/// `PUT /licenses/:id` — apply a negotiation action.
///
/// The outcome is decided by the single authoritative state machine in
/// `licensing-core::transition`; this handler owns authorization (the actor
/// must be a party to the license), fee rules (`OFFER`/`COUNTER_OFFER`
/// carry a new fee; `ACCEPT`/`REJECT` keep the current one), and the
/// atomic state + log write.
///
/// # Errors
///
/// `404` for missing/foreign licenses; `403` when the action is not for
/// the caller's role; `409` when the action is illegal from the current
/// state; `422` when an action requiring a fee lacks one.
pub async fn transition(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(license_id): Path<Uuid>,
    Json(req): Json<TransitionRequest>,
) -> ApiResult<Json<LicenseDto>> {
    let license = load_owned(&state, &user, license_id).await?;

    // Party check beyond org membership: studios act on studio-owned rows,
    // labels on their songs.
    let org = user.org_id.ok_or(ApiError::Forbidden)?;
    let role_matches_party = match user.role {
        licensing_core::Role::Studio => license.studio_id == org,
        licensing_core::Role::Label => license.label_id == org,
        licensing_core::Role::Admin => false,
    };
    if !role_matches_party {
        return Err(ApiError::NotFound);
    }

    // Fee rules: proposals carry a new fee; terminal-bound actions keep it.
    let new_fee = match req.action {
        licensing_core::LicenseAction::Offer | licensing_core::LicenseAction::CounterOffer => {
            let fee = req.license_fee_cents.ok_or_else(|| {
                ApiError::Validation(format!("{} requires license_fee_cents", req.action))
            })?;
            if fee < 0 {
                return Err(ApiError::Validation(
                    "license_fee_cents must not be negative".into(),
                ));
            }
            fee
        }
        licensing_core::LicenseAction::Accept | licensing_core::LicenseAction::Reject => {
            req.license_fee_cents
                .filter(|fee| *fee != license.license_fee_cents)
                .map_or(license.license_fee_cents, |_| {
                    // Reject explicit fee changes on non-proposal actions.
                    license.license_fee_cents
                })
        }
    };

    // THE state machine (licensing-core is the only implementation).
    let from = license.domain_state();
    let next =
        licensing_core::transition(from, req.action, user.role).map_err(|err| match err {
            licensing_core::TransitionError::ActionForbiddenForRole { .. } => ApiError::Forbidden,
            licensing_core::TransitionError::IllegalFromState { .. } => {
                ApiError::Conflict(format!("{err}"))
            }
        })?;

    let mut tx = state.pool().begin().await?;
    let row = sqlx::query_as!(
        LicenseRow,
        "UPDATE licenses
         SET state = $3, license_fee_cents = $4, updated_at = now()
         WHERE id = $1 AND state = $2
         RETURNING id, movie_id, scene_number, song_id, studio_id, label_id,
                   studio_user_id, state, license_fee_cents, start_time_seconds,
                   end_time_seconds, created_at, updated_at",
        license_id,
        from.as_str(),
        next.as_str(),
        new_fee,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::Conflict("license state changed concurrently".into()))?;

    let log_id = licensing_core::new_id();
    sqlx::query!(
        "INSERT INTO license_log
            (id, license_id, sequence, from_state, to_state, action, actor_user_id, license_fee_cents)
         SELECT $1, $2, COALESCE(MAX(sequence), 0) + 1, $3, $4, $5, $6, $7
           FROM license_log WHERE license_id = $2",
        log_id,
        license_id,
        from.as_str(),
        next.as_str(),
        req.action.as_str(),
        user.user_id,
        new_fee,
    )
    .execute(&mut *tx)
    .await?;

    let snapshot = crate::events::snapshot(&row, Some(from), &user, log_id);
    crate::events::enqueue(
        &mut *tx,
        licensing_core::LicenseEventKind::StateChanged,
        &snapshot,
    )
    .await?;

    tx.commit().await?;
    crate::events::publish_live(&state, &snapshot).await;
    Ok(Json(row.to_dto()))
}

/// Body of `PUT /licenses/:id`.
#[derive(Debug, Deserialize)]
pub struct TransitionRequest {
    /// Action to apply.
    pub action: licensing_core::LicenseAction,
    /// New fee in cents; required for `OFFER`/`COUNTER_OFFER`, ignored
    /// otherwise.
    pub license_fee_cents: Option<i64>,
}

/// `GET /licenses/:id` — detail with the lifecycle log; only the owning
/// studio or label may read.
///
/// # Errors
///
/// `404` for missing or foreign licenses.
pub async fn detail(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(license_id): Path<Uuid>,
) -> ApiResult<Json<LicenseDetailDto>> {
    let license = load_owned(&state, &user, license_id).await?;
    let log = sqlx::query_as!(
        LicenseLogRow,
        "SELECT id, license_id, sequence, from_state, to_state, action,
                actor_user_id, license_fee_cents, created_at
         FROM license_log WHERE license_id = $1 ORDER BY sequence",
        license_id,
    )
    .fetch_all(state.pool())
    .await?;

    Ok(Json(LicenseDetailDto {
        license: license.to_dto(),
        log: log.iter().map(LicenseLogRow::to_dto).collect(),
    }))
}

/// Load a license the caller is a party to; anyone else (including
/// org-less principals like admins) sees a 404 — party membership is not
/// leaked.
pub(crate) async fn load_owned(
    state: &AppState,
    user: &AuthenticatedUser,
    license_id: Uuid,
) -> ApiResult<LicenseRow> {
    let org = user.org_id.ok_or(ApiError::NotFound)?;
    sqlx::query_as!(
        LicenseRow,
        "SELECT id, movie_id, scene_number, song_id, studio_id, label_id,
                studio_user_id, state, license_fee_cents, start_time_seconds,
                end_time_seconds, created_at, updated_at
         FROM licenses
         WHERE id = $1 AND (studio_id = $2 OR label_id = $2)",
        license_id,
        org,
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or(ApiError::NotFound)
}
