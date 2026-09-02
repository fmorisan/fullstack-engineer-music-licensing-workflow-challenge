//! HTTP handlers.

pub mod auth;
pub mod verify;

use crate::error::{ApiError, ApiResult};
use crate::jwt;
use crate::models::UserRow;
use crate::state::AppState;
use axum::http::HeaderMap;

/// Authenticate a request from its `Authorization: Bearer` header and load
/// the user row.
///
/// # Errors
///
/// [`ApiError::Unauthorized`] when the header is missing, the token is
/// invalid/expired, or the user no longer exists.
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> ApiResult<UserRow> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;
    let token = jwt::bearer_token(header).ok_or(ApiError::Unauthorized)?;
    let claims = jwt::decode(state.jwt_secret(), token).map_err(|_| ApiError::Unauthorized)?;
    let user_id: uuid::Uuid = claims.sub.parse().map_err(|_| ApiError::Unauthorized)?;

    let user = sqlx::query_as!(
        UserRow,
        r#"SELECT id, email, password_hash, display_name, role, org_id, created_at, updated_at
           FROM users WHERE id = $1"#,
        user_id
    )
    .fetch_optional(state.pool())
    .await?
    .ok_or(ApiError::Unauthorized)?;
    Ok(user)
}
