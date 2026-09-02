//! API error mapping to HTTP responses.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors surfaced by handlers.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Request payload failed validation.
    #[error("{0}")]
    Validation(String),
    /// Missing or invalid credentials/token.
    #[error("unauthorized")]
    Unauthorized,
    /// Authenticated but not permitted.
    #[error("forbidden")]
    Forbidden,
    /// Resource already exists.
    #[error("{0}")]
    Conflict(String),
    /// Unexpected internal failure; details are logged, not leaked.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".to_string()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            ApiError::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        let body = Json(serde_json::json!({
            "error": { "code": status.as_u16(), "message": message }
        }));
        (status, body).into_response()
    }
}

/// Handler result alias.
pub type ApiResult<T> = Result<T, ApiError>;
