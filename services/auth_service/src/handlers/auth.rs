//! Registration, login, profile, and JWKS endpoints.

use axum::Extension;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use licensing_core::Role;
use platform::AuthenticatedUser;
use serde::Deserialize;

use crate::cookies;
use crate::error::{ApiError, ApiResult};
use crate::handlers::refresh as refresh_handler;
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

/// Successful auth response: the user plus a freshly minted access token.
#[derive(Debug, serde::Serialize)]
pub struct AuthResponse {
    /// Public user view.
    pub user: UserDto,
    /// RS256 bearer token.
    pub access_token: String,
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
/// user plus a refresh session in one transaction, returning `201` with
/// user + access token and the refresh cookie.
///
/// # Errors
///
/// `422` on validation failures, `409` on duplicate email.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<axum::response::Response> {
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

    let refresh = refresh_handler::issue(
        &mut *tx,
        user.id,
        state.refresh_ttl_secs(),
        state.cookie_secure(),
    )
    .await
    .map_err(ApiError::from)?;

    tx.commit().await.map_err(ApiError::from)?;

    let access_token = mint_access_token(&state, &user)?;
    let mut response = (
        StatusCode::CREATED,
        Json(AuthResponse {
            user: user.to_dto(),
            access_token,
        }),
    )
        .into_response();
    cookies::attach(&mut response, &refresh.set_cookie);
    Ok(response)
}

/// `POST /auth/login`
///
/// # Errors
///
/// `401` for unknown emails and wrong passwords alike.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<axum::response::Response> {
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
            let refresh = refresh_handler::issue(
                state.pool(),
                user.id,
                state.refresh_ttl_secs(),
                state.cookie_secure(),
            )
            .await
            .map_err(ApiError::from)?;
            let access_token = mint_access_token(&state, &user)?;
            let mut response = Json(AuthResponse {
                user: user.to_dto(),
                access_token,
            })
            .into_response();
            cookies::attach(&mut response, &refresh.set_cookie);
            Ok(response)
        }
        // Burn a comparable amount of time for unknown emails so login
        // latency does not reveal account existence.
        Some(_) | None => {
            let _ = password::hash(&req.password);
            Err(ApiError::Unauthorized)
        }
    }
}

/// `GET /auth/me` — identity comes from the platform auth middleware.
///
/// # Errors
///
/// `401` when the principal no longer matches a persisted user.
pub async fn me(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<Json<UserDto>> {
    let row = sqlx::query_as!(
        UserRow,
        r#"SELECT id, email, password_hash, display_name, role, org_id, created_at, updated_at
           FROM users WHERE id = $1"#,
        user.user_id,
    )
    .fetch_optional(state.pool())
    .await
    .map_err(ApiError::from)?
    .ok_or(ApiError::Unauthorized)?;
    Ok(Json(row.to_dto()))
}

/// `GET /.well-known/jwks.json` — public signing keys (ADR-011).
///
/// Consumed by production edge proxies (Traefik Hub `jwksUrl`, Kong
/// Enterprise, Envoy) and anyone wanting asymmetric verification.
pub async fn jwks(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.signing().jwks.clone())
}

/// Mint an access token for the given user.
pub(crate) fn mint_access_token(state: &AppState, user: &UserRow) -> ApiResult<String> {
    let now = Utc::now();
    let claims = platform::jwt::Claims {
        sub: user.id.to_string(),
        role: user.domain_role(),
        org_id: user.org_id,
        iss: platform::jwt::ISSUER.to_string(),
        iat: now.timestamp(),
        exp: now.timestamp() + state.jwt_expiry_secs(),
    };
    platform::jwt::encode(&state.signing().private_pem, &claims)
        .map_err(|err| ApiError::Internal(err.into()))
}
