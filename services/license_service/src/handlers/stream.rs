//! `GET /licenses/stream` — live license updates over Server-Sent Events
//! (ADR-005). EventSource clients authenticate via `?access_token=` (the
//! platform middleware accepts it); payloads are post-transition
//! `LicenseSnapshot` documents.

use std::convert::Infallible;

use axum::Extension;
use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event as SseEvent, KeepAlive};
use platform::AuthenticatedUser;
use tokio_stream::Stream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /licenses/stream` — live license updates over Server-Sent Events
/// (ADR-005). EventSource clients authenticate via `?access_token=` (the
/// platform middleware accepts it); payloads are post-transition
/// `LicenseSnapshot` documents, **filtered to licenses the caller's org is
/// a party to** — the shared channel carries every update, and cross-tenant
/// snapshots must never reach a connected client.
///
/// # Errors
///
/// `500` when live wiring is absent (only in misconfigured deployments).
pub async fn stream(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    let fanout = state.fanout().ok_or(ApiError::Internal(anyhow::anyhow!(
        "live updates not configured"
    )))?;
    let rx = fanout.subscribe();
    let allowed_org = user.org_id;

    let events = BroadcastStream::new(rx).filter_map(move |message| match message {
        Ok(payload) => {
            let visible = serde_json::from_str::<serde_json::Value>(payload.as_ref())
                .ok()
                .is_some_and(|snapshot| {
                    allowed_org.is_some_and(|org| {
                        snapshot["studio_id"].as_str() == Some(org.to_string().as_str())
                            || snapshot["label_id"].as_str() == Some(org.to_string().as_str())
                    })
                });
            visible.then(|| {
                Ok(SseEvent::default()
                    .event("license-updated")
                    .data(payload.as_ref()))
            })
        }
        Err(_lagged) => {
            // The client fell behind; it will reconcile by refetching.
            None
        }
    });

    // Live-connection gauge: the guard rides the stream and decrements
    // when the client disconnects.
    let connection = platform::metrics::sse_connection("licenses");
    let events = events.map(move |event| {
        let _held = &connection;
        event
    });

    Ok(Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
