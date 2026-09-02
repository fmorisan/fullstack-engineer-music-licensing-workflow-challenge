//! Authentication middleware and principal extraction (ADR-011).
//!
//! Every service mounts [`require_auth`] with the issuer's public key
//! ([`JwtAuth`]). On success the [`AuthenticatedUser`] principal lands in the
//! request extensions; handlers declare it as an [`axum::Extension`] and
//! enforce role rules with [`AuthenticatedUser::ensure_role`].
//!
//! Tokens travel as `Authorization: Bearer <token>`, or as an `access_token`
//! query parameter for clients that cannot set headers (EventSource streams).

use std::sync::Arc;

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use licensing_core::Role;
use uuid::Uuid;

use crate::jwt::Claims;
use crate::jwt::JwtError;

/// The authenticated principal, inserted by [`require_auth`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Authenticated user id.
    pub user_id: Uuid,
    /// Role from the verified claims.
    pub role: Role,
    /// Organization id, when the role belongs to one.
    pub org_id: Option<Uuid>,
}

impl From<Claims> for AuthenticatedUser {
    fn from(claims: Claims) -> Self {
        Self {
            user_id: claims.sub.parse().unwrap_or_default(),
            role: claims.role,
            org_id: claims.org_id,
        }
    }
}

impl AuthenticatedUser {
    /// Enforce a role requirement at the handler level.
    ///
    /// # Errors
    ///
    /// [`AuthError::Forbidden`] when the role does not match.
    pub fn ensure_role(&self, required: Role) -> Result<(), AuthError> {
        if self.role == required {
            Ok(())
        } else {
            Err(AuthError::Forbidden)
        }
    }
}

/// Shared verification key for [`require_auth`].
#[derive(Clone)]
pub struct JwtAuth {
    public_pem: Arc<str>,
}

impl JwtAuth {
    /// Wrap a public key PEM (SubjectPublicKeyInfo format).
    pub fn from_public_key_pem(pem: impl Into<String>) -> Self {
        Self {
            public_pem: Arc::from(pem.into()),
        }
    }

    /// Read the public key PEM from a file.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read.
    pub fn from_public_key_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let pem = std::fs::read_to_string(path)?;
        Ok(Self::from_public_key_pem(pem))
    }

    /// Verify a token against the key.
    ///
    /// # Errors
    ///
    /// Propagates [`JwtError`] on invalid or expired tokens.
    pub fn verify(&self, token: &str) -> Result<Claims, JwtError> {
        crate::jwt::decode(&self.public_pem, token)
    }
}

/// Errors surfaced by the auth middleware.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Missing, malformed, or invalid token.
    #[error("unauthorized")]
    Unauthorized,
    /// Authenticated principal lacks the required role.
    #[error("forbidden")]
    Forbidden,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        };
        let body = Json(serde_json::json!({
            "error": { "code": status.as_u16(), "message": message }
        }));
        (status, body).into_response()
    }
}

fn bearer_token(req: &Request) -> Option<&str> {
    if let Some(header) = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && let Some(token) = header.strip_prefix("Bearer ").filter(|t| !t.is_empty())
    {
        return Some(token);
    }
    // EventSource-style fallback: ?access_token=...
    req.uri().query().and_then(|query| {
        query.split('&').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == "access_token").then_some(value)
        })
    })
}

/// Middleware: verify the bearer token and insert the [`AuthenticatedUser`]
/// principal into the request extensions.
///
/// Mount with `middleware::from_fn_with_state(jwt_auth, require_auth)` and
/// `with_state(jwt_auth)`.
///
/// # Errors
///
/// Returns [`AuthError::Unauthorized`] as a 401 response for missing or
/// invalid tokens.
pub async fn require_auth(
    axum::extract::State(auth): axum::extract::State<JwtAuth>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&req).ok_or(AuthError::Unauthorized)?;
    let claims = auth.verify(token).map_err(|_| AuthError::Unauthorized)?;
    req.extensions_mut().insert(AuthenticatedUser::from(claims));
    Ok(next.run(req).await)
}
