//! `GET /licenses/stream` — live license updates over Server-Sent Events
//! (ADR-005). EventSource clients authenticate via `?access_token=` (the
//! platform middleware accepts it); payloads are post-transition
//! `LicenseSnapshot` documents.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event as SseEvent, KeepAlive};
use tokio_stream::Stream;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// SSE stream of license updates.
///
/// # Errors
///
/// `503` when live wiring is absent (only in misconfigured deployments).
pub async fn stream(
    State(state): State<AppState>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    let fanout = state.fanout().ok_or(ApiError::Internal(anyhow::anyhow!(
        "live updates not configured"
    )))?;
    let rx = fanout.subscribe();

    let events = BroadcastStream::new(rx).filter_map(|message| match message {
        Ok(payload) => Some(Ok(SseEvent::default()
            .event("license-updated")
            .data(payload.as_ref()))),
        Err(_lagged) => {
            // The client fell behind; it will reconcile by refetching.
            None
        }
    });

    Ok(Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
