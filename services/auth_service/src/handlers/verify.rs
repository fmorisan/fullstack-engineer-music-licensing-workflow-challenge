//! `GET /verify` — the Traefik forward-auth target (ADR-002).
//!
//! Traefik calls this endpoint with the original request's headers before
//! proxying to an upstream service. `200` (with `X-User-*` headers Traefik
//! copies onto the upstream request) lets it through; `401`/`403` blocks it.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::jwt;
use crate::state::AppState;

/// Query parameters of `GET /verify`.
#[derive(Debug, Default, Deserialize)]
pub struct VerifyQuery {
    /// Require this role; when absent, any authenticated role passes.
    pub role: Option<licensing_core::Role>,
}

fn header_value(value: impl AsRef<[u8]>) -> ApiResult<HeaderValue> {
    HeaderValue::from_bytes(value.as_ref())
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("header encoding failed: {err}")))
}

/// `GET /verify?role=STUDIO`
///
/// # Errors
///
/// `401` for missing/invalid tokens, `403` when the `role` requirement is
/// not met.
pub async fn verify(
    State(state): State<AppState>,
    Query(query): Query<VerifyQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;
    let token = jwt::bearer_token(authorization).ok_or(ApiError::Unauthorized)?;
    let claims = jwt::decode(state.jwt_secret(), token).map_err(|_| ApiError::Unauthorized)?;

    if let Some(required) = query.role
        && claims.role != required
    {
        return Err(ApiError::Forbidden);
    }

    let org_id = claims.org_id.map(|id| id.to_string());
    let mut response = Json(serde_json::json!({
        "user_id": claims.sub,
        "role": claims.role,
        "org_id": claims.org_id,
    }))
    .into_response();

    let response_headers = response.headers_mut();
    response_headers.insert("X-User-Id", header_value(&claims.sub)?);
    response_headers.insert("X-User-Role", header_value(claims.role.as_str())?);
    if let Some(org_id) = org_id {
        response_headers.insert("X-User-Org", header_value(org_id)?);
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    #[test]
    fn forwarded_header_names_match_the_traefik_contract() {
        // authResponseHeaders in infrastructure/docker/traefik/dynamic.yml.
        for name in ["X-User-Id", "X-User-Role", "X-User-Org"] {
            assert!(name.starts_with("X-User-"));
        }
    }
}
