//! Refresh session endpoints: `POST /auth/refresh`, `POST /auth/logout`
//! (ADR-012).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::cookies;
use crate::error::{ApiError, ApiResult};
use crate::handlers::auth::AuthResponse;
use crate::models::UserRow;
use crate::refresh;
use crate::state::AppState;

/// A freshly issued refresh token plus its Set-Cookie value.
pub struct IssuedRefresh {
    /// Opaque token value (only its hash is persisted).
    pub raw: String,
    /// Ready-to-attach `Set-Cookie` header value.
    pub set_cookie: String,
    /// Row id of the inserted token.
    pub row_id: Uuid,
}

/// Insert a new refresh token row (start of a new family).
///
/// # Errors
///
/// Propagates database errors.
pub async fn issue<'e, E>(
    executor: E,
    user_id: Uuid,
    ttl_secs: i64,
    cookie_secure: bool,
) -> sqlx::Result<IssuedRefresh>
where
    E: PgExecutor<'e>,
{
    let raw = refresh::generate();
    let row_id = licensing_core::new_id();
    let expires_at = Utc::now() + Duration::seconds(ttl_secs);
    sqlx::query!(
        "INSERT INTO refresh_tokens (id, user_id, family_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
        row_id,
        user_id,
        licensing_core::new_id(),
        refresh::hash(&raw),
        expires_at,
    )
    .execute(executor)
    .await?;
    let set_cookie = cookies::set_cookie(&raw, ttl_secs, cookie_secure);
    Ok(IssuedRefresh {
        raw,
        set_cookie,
        row_id,
    })
}

/// Rotate a valid refresh token within one transaction: mark the old row,
/// insert the replacement in the same family, purge expired rows.
async fn rotate(
    state: &AppState,
    current: &CurrentToken,
    ttl_secs: i64,
    cookie_secure: bool,
) -> sqlx::Result<IssuedRefresh> {
    let raw = refresh::generate();
    let row_id = licensing_core::new_id();
    let expires_at = Utc::now() + Duration::seconds(ttl_secs);

    let mut tx = state.pool().begin().await?;
    sqlx::query!(
        "UPDATE refresh_tokens SET rotated_at = now(), replaced_by = $2 WHERE id = $1",
        current.id,
        row_id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO refresh_tokens (id, user_id, family_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
        row_id,
        current.user_id,
        current.family_id,
        refresh::hash(&raw),
        expires_at,
    )
    .execute(&mut *tx)
    .await?;
    // Lazy purge: expired rows for this user never outlive an active session.
    sqlx::query!(
        "DELETE FROM refresh_tokens WHERE user_id = $1 AND expires_at < now()",
        current.user_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let set_cookie = cookies::set_cookie(&raw, ttl_secs, cookie_secure);
    Ok(IssuedRefresh {
        raw,
        set_cookie,
        row_id,
    })
}

/// The refresh-token row matching a presented cookie.
struct CurrentToken {
    id: Uuid,
    user_id: Uuid,
    family_id: Uuid,
    rotated_at: Option<chrono::DateTime<Utc>>,
    revoked_at: Option<chrono::DateTime<Utc>>,
    expires_at: chrono::DateTime<Utc>,
}

async fn load(state: &AppState, raw: &str) -> sqlx::Result<Option<CurrentToken>> {
    let row = sqlx::query!(
        "SELECT id, user_id, family_id, rotated_at, revoked_at, expires_at
         FROM refresh_tokens WHERE token_hash = $1",
        refresh::hash(raw),
    )
    .fetch_optional(state.pool())
    .await?;
    Ok(row.map(|row| CurrentToken {
        id: row.id,
        user_id: row.user_id,
        family_id: row.family_id,
        rotated_at: row.rotated_at,
        revoked_at: row.revoked_at,
        expires_at: row.expires_at,
    }))
}

/// Revoke every token in a family.
async fn revoke_family(state: &AppState, family_id: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked_at = now()
         WHERE family_id = $1 AND revoked_at IS NULL",
        family_id,
    )
    .execute(state.pool())
    .await?;
    Ok(())
}

/// `POST /auth/refresh` — exchange the refresh cookie for a new access token
/// and a rotated refresh cookie.
///
/// # Errors
///
/// `401` for missing/expired/revoked/rotated tokens; presenting an
/// already-rotated token additionally revokes its whole family.
pub async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    let raw = cookies::refresh_from_headers(&headers).ok_or(ApiError::Unauthorized)?;
    let current = load(&state, &raw)
        .await
        .map_err(ApiError::from)?
        .ok_or(ApiError::Unauthorized)?;

    if current.revoked_at.is_some() {
        return Err(ApiError::Unauthorized);
    }
    if current.rotated_at.is_some() {
        // Replay of a one-time token: assume theft, kill the family.
        tracing::warn!(
            family = %current.family_id,
            "rotated refresh token replayed; revoking family"
        );
        revoke_family(&state, current.family_id)
            .await
            .map_err(ApiError::from)?;
        return Err(ApiError::Unauthorized);
    }
    if current.expires_at <= Utc::now() {
        return Err(ApiError::Unauthorized);
    }

    let user = sqlx::query_as!(
        UserRow,
        "SELECT id, email, password_hash, display_name, role, org_id, created_at, updated_at
         FROM users WHERE id = $1",
        current.user_id,
    )
    .fetch_optional(state.pool())
    .await
    .map_err(ApiError::from)?
    .ok_or(ApiError::Unauthorized)?;

    let issued = rotate(
        &state,
        &current,
        state.refresh_ttl_secs(),
        state.cookie_secure(),
    )
    .await
    .map_err(ApiError::from)?;

    let token = super::auth::mint_access_token(&state, &user)?;
    let mut response = Json(AuthResponse {
        user: user.to_dto(),
        access_token: token,
    })
    .into_response();
    cookies::attach(&mut response, &issued.set_cookie);
    Ok(response)
}

/// `POST /auth/logout` — end the session: revoke the token family and clear
/// the cookie. Access tokens are not revocable and expire within 15 minutes
/// (ADR-012).
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(raw) = cookies::refresh_from_headers(&headers)
        && let Ok(Some(current)) = load(&state, &raw).await
        && current.revoked_at.is_none()
    {
        let _ = revoke_family(&state, current.family_id).await;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    cookies::attach(&mut response, &cookies::clear_cookie(state.cookie_secure()));
    response
}
