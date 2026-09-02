//! Registration, login, and profile endpoints.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use licensing_core::Role;
use serde::Deserialize;

use super::authenticate;
use crate::error::{ApiError, ApiResult};
use crate::jwt;
use crate::models::{UserDto, UserRow};
use crate::password;
use crate::state::AppState;

/// Body of `POST /auth/register`.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Login email.
    pub email: String,
    /// Plaintext password; hashed immediately, never stored.
    pub password: String,
    /// Human-friendly name.
    pub display_name: String,
    /// Requested role.
    pub role: Role,
    /// Organization name; required for STUDIO/LABEL, rejected for ADMIN.
    pub org_name: Option<String>,
}

/// Body of `POST /auth/login`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Login email.
    pub email: String,
    /// Plaintext password.
    pub password: String,
}

/// Successful auth response: the user plus a freshly minted token.
#[derive(Debug, serde::Serialize)]
pub struct AuthResponse {
    /// Public user view.
    pub user: UserDto,
    /// Bearer token.
    pub token: String,
}

/// An email shape we accept: something@something, no spaces.
fn plausible_email(email: &str) -> bool {
    let (local, domain) = email.split_once('@').map_or(("", ""), |(l, d)| (l, d));
    !local.is_empty() && domain.contains('.') && !email.contains(char::is_whitespace)
}

fn map_unique_violation(err: sqlx::Error, message: &str) -> ApiError {
    let unique = err
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation);
    if unique {
        ApiError::Conflict(message.to_string())
    } else {
        ApiError::Internal(err.into())
    }
}

/// `POST /auth/register`
///
/// Creates the organization on first use (idempotent on name+kind) and the
/// user in one transaction, returning `201` with user + token.
///
/// # Errors
///
/// `422` on validation failures, `409` on duplicate email.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<AuthResponse>)> {
    let email = req.email.trim().to_ascii_lowercase();
    let display_name = req.display_name.trim().to_string();
    let org_name = req
        .org_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if !plausible_email(&email) {
        return Err(ApiError::Validation("email is not a valid address".into()));
    }
    if req.password.len() < 8 {
        return Err(ApiError::Validation(
            "password must be at least 8 characters".into(),
        ));
    }
    if display_name.is_empty() {
        return Err(ApiError::Validation(
            "display_name must not be empty".into(),
        ));
    }

    let org_name = match req.role {
        Role::Admin => {
            if org_name.is_some() {
                return Err(ApiError::Validation(
                    "org_name must not be provided for ADMIN".into(),
                ));
            }
            None
        }
        Role::Studio | Role::Label => Some(org_name.ok_or_else(|| {
            ApiError::Validation(format!("org_name is required for {}", req.role))
        })?),
    };
    if org_name.as_ref().is_some_and(|n| n.len() > 120) {
        return Err(ApiError::Validation(
            "org_name must be at most 120 characters".into(),
        ));
    }

    let password_hash = password::hash(&req.password).map_err(ApiError::Internal)?;
    let user_id = licensing_core::new_id();

    let mut tx = state.pool().begin().await.map_err(ApiError::from)?;

    let org_id = match &org_name {
        None => None,
        Some(name) => {
            let id = sqlx::query_scalar!(
                r#"INSERT INTO organizations (id, name, kind)
                   VALUES ($1, $2, $3)
                   ON CONFLICT (name, kind) DO UPDATE SET name = EXCLUDED.name
                   RETURNING id"#,
                licensing_core::new_id(),
                name,
                req.role.as_str(),
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| map_unique_violation(e, "organization conflict"))?;
            Some(id)
        }
    };

    let user = sqlx::query_as!(
        UserRow,
        r#"INSERT INTO users (id, email, password_hash, display_name, role, org_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, email, password_hash, display_name, role, org_id, created_at, updated_at"#,
        user_id,
        email,
        password_hash,
        display_name,
        req.role.as_str(),
        org_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| map_unique_violation(e, "email already registered"))?;

    tx.commit().await.map_err(ApiError::from)?;

    let token = mint(&state, &user, Utc::now())?;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user: user.to_dto(),
            token,
        }),
    ))
}

/// `POST /auth/login`
///
/// # Errors
///
/// `401` for unknown emails and wrong passwords alike.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let email = req.email.trim().to_ascii_lowercase();

    let user = sqlx::query_as!(
        UserRow,
        r#"SELECT id, email, password_hash, display_name, role, org_id, created_at, updated_at
           FROM users WHERE email = $1"#,
        email,
    )
    .fetch_optional(state.pool())
    .await
    .map_err(ApiError::from)?;

    match user {
        Some(user) if password::verify(&req.password, &user.password_hash) => {
            let token = mint(&state, &user, Utc::now())?;
            Ok(Json(AuthResponse {
                user: user.to_dto(),
                token,
            }))
        }
        // Burn a comparable amount of time for unknown emails so login
        // latency does not reveal account existence.
        Some(_) | None => {
            let _ = password::hash(&req.password);
            Err(ApiError::Unauthorized)
        }
    }
}

/// `GET /auth/me`
///
/// # Errors
///
/// `401` without a valid bearer token.
pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<UserDto>> {
    let user = authenticate(&state, &headers).await?;
    Ok(Json(user.to_dto()))
}

fn mint(state: &AppState, user: &UserRow, now: DateTime<Utc>) -> ApiResult<String> {
    jwt::encode(
        state.jwt_secret(),
        user.id,
        user.domain_role(),
        user.org_id,
        now,
        state.jwt_expiry_secs(),
    )
    .map_err(ApiError::from)
}
