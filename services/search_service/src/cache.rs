//! Redis cache-aside for search results and song details (ADR: search stack).
//!
//! Worst-case staleness is bounded by the TTL plus Kafka lag. Popularity is
//! tracked per query in a ZSET for future tuning; it counts requests, not
//! distinct users.

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

/// ZSET tracking query popularity (score = request count).
const HOT_QUERIES_KEY: &str = "search:hot";

/// Cap on the query fragment embedded in cache keys.
const KEY_QUERY_MAX: usize = 80;

/// Cache key for a search page.
#[must_use]
pub fn search_key(query: &str, size: i64, from: i64) -> String {
    let normalized = query.trim().to_lowercase();
    let fragment = if normalized.len() > KEY_QUERY_MAX {
        &normalized[..KEY_QUERY_MAX]
    } else {
        &normalized
    };
    format!("search:q:{fragment}:s:{size}:f:{from}")
}

/// Cache key for a song detail document.
#[must_use]
pub fn detail_key(song_id: uuid::Uuid) -> String {
    format!("search:song:{song_id}")
}

/// Fetch a cached JSON value.
///
/// # Errors
///
/// Propagates redis errors; callers treat them as cache misses.
pub async fn get_cached(
    redis: &ConnectionManager,
    key: &str,
) -> redis::RedisResult<Option<serde_json::Value>> {
    let raw: Option<String> = redis.clone().get(key).await?;
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json).unwrap_or_default())),
        None => Ok(None),
    }
}

/// Store a JSON value with a TTL.
///
/// # Errors
///
/// Propagates redis errors; callers ignore failures (cache is best-effort).
pub async fn set_cached(
    redis: &ConnectionManager,
    key: &str,
    value: &serde_json::Value,
    ttl_secs: u64,
) -> redis::RedisResult<()> {
    let json = serde_json::to_string(value).unwrap_or_default();
    redis.clone().set_ex(key, json, ttl_secs).await
}

/// Record one request for `query` in the popularity ZSET.
///
/// # Errors
///
/// Propagates redis errors; callers ignore failures.
pub async fn bump_popularity(redis: &ConnectionManager, query: &str) -> redis::RedisResult<()> {
    let normalized = query.trim().to_lowercase();
    let fragment = &normalized[..normalized.len().min(KEY_QUERY_MAX)];
    redis
        .clone()
        .zincr(HOT_QUERIES_KEY, fragment, 1.0)
        .await
        .map(|_: f64| ())
}

/// Read the popularity score of a query (tests/diagnostics).
///
/// # Errors
///
/// Propagates redis errors.
pub async fn popularity(redis: &ConnectionManager, query: &str) -> redis::RedisResult<f64> {
    let normalized = query.trim().to_lowercase();
    let fragment = &normalized[..normalized.len().min(KEY_QUERY_MAX)];
    let score: Option<f64> = redis.clone().zscore(HOT_QUERIES_KEY, fragment).await?;
    Ok(score.unwrap_or(0.0))
}

/// Remaining TTL of a key in seconds (tests/diagnostics).
///
/// # Errors
///
/// Propagates redis errors.
pub async fn ttl(redis: &ConnectionManager, key: &str) -> redis::RedisResult<i64> {
    redis.clone().ttl(key).await
}
