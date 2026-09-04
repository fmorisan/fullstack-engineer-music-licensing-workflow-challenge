//! Search and detail endpoints.

use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::header::HeaderName;
use axum::response::IntoResponse;
use platform::AuthenticatedUser;
use serde::Deserialize;

use crate::cache;
use crate::error::{ApiError, ApiResult};
use crate::es;
use crate::state::AppState;

/// Query parameters of `GET /songs/search`.

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Free-text query; fuzzy-matched over title (boosted) and author, with
    /// autocomplete prefix matching.
    pub q: String,
    /// Page size (1-50, default 10).
    pub size: Option<i64>,
    /// Page offset (0-1000, default 0).
    pub from: Option<i64>,
}

/// `X-Cache` header value for observability.
fn with_cache_header(response: &mut axum::response::Response, hit: bool) -> Result<(), ApiError> {
    let value = if hit { "hit" } else { "miss" };
    response.headers_mut().insert(
        HeaderName::from_static("x-cache"),
        value
            .parse()
            .map_err(|err| ApiError::Internal(anyhow::anyhow!("header encoding failed: {err}")))?,
    );
    Ok(())
}

/// `GET /songs/search?q=` — fuzzy + autocomplete catalog search, served
/// cache-aside with a short TTL (worst-case staleness ≈ TTL + index lag).
///
/// # Errors
///
/// `422` on invalid parameters; `500` when the backing stores fail.
pub async fn search(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthenticatedUser>,
    Query(params): Query<SearchParams>,
) -> ApiResult<axum::response::Response> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err(ApiError::Validation("q must not be empty".into()));
    }
    if query.len() > 200 {
        return Err(ApiError::Validation(
            "q must be at most 200 characters".into(),
        ));
    }
    let size = params.size.unwrap_or(10).clamp(1, 50);
    let from = params.from.unwrap_or(0).clamp(0, 1000);

    let key = cache::search_key(query, size, from);

    // Cache read (miss on any redis hiccup).
    let cached = cache::get_cached(state.redis(), &key).await.unwrap_or(None);

    let page = match cached {
        Some(page) => {
            let mut response = Json(page).into_response();
            with_cache_header(&mut response, true)?;
            // Popularity counts requests, cached or not.
            let _ = cache::bump_popularity(state.redis(), query).await;
            return Ok(response);
        }
        None => es::search(state.es(), query, from, size).await?,
    };

    let _ = cache::set_cached(
        state.redis(),
        &key,
        &serde_json::to_value(&page).unwrap_or_default(),
        state.search_cache_ttl_secs(),
    )
    .await;
    let _ = cache::bump_popularity(state.redis(), query).await;

    let mut response = Json(page).into_response();
    with_cache_header(&mut response, false)?;
    Ok(response)
}

/// Query parameters of `GET /songs/newest`.
#[derive(Debug, Deserialize)]
pub struct NewestParams {
    /// Page size (1-50, default 8).
    pub size: Option<i64>,
}

/// `GET /songs/newest` — newest-first catalog slice powering pre-search
/// suggestions; cache-aside like search (worst-case staleness ≈ TTL +
/// index lag).
///
/// # Errors
///
/// `500` when the backing stores fail.
pub async fn newest(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthenticatedUser>,
    Query(params): Query<NewestParams>,
) -> ApiResult<axum::response::Response> {
    let size = params.size.unwrap_or(8).clamp(1, 50);
    let key = cache::newest_key(size);

    let cached = cache::get_cached(state.redis(), &key).await.unwrap_or(None);
    let page = match cached {
        Some(page) => {
            let mut response = Json(page).into_response();
            with_cache_header(&mut response, true)?;
            return Ok(response);
        }
        None => es::newest(state.es(), size).await?,
    };

    let _ = cache::set_cached(
        state.redis(),
        &key,
        &serde_json::to_value(&page).unwrap_or_default(),
        state.search_cache_ttl_secs(),
    )
    .await;

    let mut response = Json(page).into_response();
    with_cache_header(&mut response, false)?;
    Ok(response)
}

/// One trending query with its request count.
#[derive(Debug, serde::Serialize)]
pub struct HotQuery {
    /// The normalized query string.
    pub query: String,
    /// Total requests for this query (cached or not).
    pub score: f64,
}

/// Query parameters of `GET /songs/hot-queries`.
#[derive(Debug, Deserialize)]
pub struct HotQueriesParams {
    /// How many queries to return (1-25, default 8).
    pub limit: Option<usize>,
}

/// `GET /songs/hot-queries` — queries ranked by search volume from the
/// popularity ZSET, powering the trending-search chips.
///
/// # Errors
///
/// `500` when Redis fails (trending is the product of the cache; a stale
/// fallback would lie).
pub async fn hot_queries(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthenticatedUser>,
    Query(params): Query<HotQueriesParams>,
) -> ApiResult<Json<Vec<HotQuery>>> {
    let limit = params.limit.unwrap_or(8).clamp(1, 25);
    let rows = cache::top_queries(state.redis(), limit)
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;
    Ok(Json(
        rows.into_iter()
            .map(|(query, score)| HotQuery { query, score })
            .collect(),
    ))
}
