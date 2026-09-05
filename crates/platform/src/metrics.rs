//! Prometheus metrics for every service (ADR-014).
//!
//! One global recorder per process, installed lazily on first use. HTTP
//! middleware records per-route request counts and latencies; pipeline
//! helpers (consumers, outbox relay, SSE gauges, supervisor restarts) are
//! incremented where the events happen. `/metrics` renders the exposition
//! format; healthz and metrics themselves stay uninstrumented so scrapes
//! never pollute the signal.

use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::MatchedPath;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::PrometheusBuilder;
use metrics_exporter_prometheus::PrometheusHandle;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global recorder (idempotent) and return its handle.
///
/// # Panics
///
/// Panics if the recorder cannot install — a process-startup failure.
#[must_use]
pub fn exporter() -> &'static PrometheusHandle {
    HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("prometheus recorder installs exactly once per process")
    })
}

/// Axum middleware: `http_requests_total{method,route,status}` and
/// `http_request_duration_seconds{method,route}` histograms. Apply with
/// `Router::layer` so routing has already resolved [`MatchedPath`] — add
/// the `/metrics` route *after* layering to keep scrapes unmeasured.
pub async fn http_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let _exporter = exporter();
    let method = req.method().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_string(), |path| path.as_str().to_string());
    let started = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status,
    )
    .increment(1);
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "route" => route,
    )
    .record(started.elapsed().as_secs_f64());
    response
}

/// `GET /metrics` — the Prometheus exposition endpoint.
#[allow(clippy::unused_async)] // handlers plug into axum's async routing
pub async fn render() -> Response {
    let body = exporter().render();
    let mut response = Response::new(axum::body::Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    *response.status_mut() = StatusCode::OK;
    response
}

/// Record one consumed Kafka message (called by [`crate::kafka`]).
pub(crate) fn record_consumed(group: &str) {
    metrics::counter!("events_consumed_total", "group" => group.to_string()).increment(1);
}

/// Record one outbox row published to Kafka.
pub fn record_outbox_published(count: u64) {
    metrics::counter!("outbox_published_total").increment(count);
}

/// Record a consumer client rebuild (the zombie guard tripped).
pub fn record_consumer_rebuild(reason: &str) {
    metrics::counter!("consumer_rebuilds_total", "reason" => reason.to_string()).increment(1);
}

/// One more live SSE client on this stream.
pub fn sse_connected(stream: &str) {
    metrics::gauge!("sse_connected", "stream" => stream.to_string()).increment(1.0);
}

/// One fewer live SSE client.
pub fn sse_disconnected(stream: &str) {
    metrics::gauge!("sse_connected", "stream" => stream.to_string()).decrement(1.0);
}

/// RAII connection tracker: increment on create, decrement when the
/// stream (which owns the guard) drops because the client disconnected.
pub struct SseConnectionGuard {
    stream: String,
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        sse_disconnected(&self.stream);
    }
}

/// Track a live SSE connection for `stream`.
#[must_use]
pub fn sse_connection(stream: &str) -> SseConnectionGuard {
    sse_connected(stream);
    SseConnectionGuard {
        stream: stream.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    async fn healthz() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn middleware_counts_routes_and_renders() {
        let app = Router::new()
            .route("/healthz", get(healthz))
            .layer(axum::middleware::from_fn(http_middleware))
            .route("/metrics", get(render));

        let response = app
            .clone()
            .oneshot(
                Request::get("/healthz")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::get("/metrics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(content_type.starts_with("text/plain"));
        assert!(
            body.contains("http_requests_total")
                && body.contains(r#"route="/healthz""#)
                && body.contains("http_request_duration_seconds"),
            "rendered metrics must carry the request series: {body}"
        );
        // Scrapes stay unmeasured: /metrics is layered out.
        assert!(!body.contains(r#"route="/metrics""#));
    }
}
