//! Inbox endpoints: list, unread count, read markers, and the live SSE
//! stream (ADR-006).

use std::convert::Infallible;

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Sse;
use axum::response::sse::{Event as SseEvent, KeepAlive};
use platform::AuthenticatedUser;
use serde::Deserialize;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{NotificationDto, NotificationRow};
use crate::state::AppState;

/// Query of `GET /notifications`.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// Page size (1-100, default 20).
    pub limit: Option<i64>,
    /// Page offset.
    pub offset: Option<i64>,
    /// When `true`, only unread rows.
    pub unread: Option<bool>,
}

/// `GET /notifications` — the caller's org inbox, newest first.
///
/// # Errors
///
/// `403` for org-less principals (admins).
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<Vec<NotificationDto>>> {
    let org = user.org_id.ok_or(ApiError::Forbidden)?;
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).clamp(0, 10_000);

    let rows = if params.unread.unwrap_or(false) {
        sqlx::query_as::<_, NotificationRow>(
            "SELECT id, recipient_user_id, recipient_org_id, type, payload,
                    source_event_id, read_at, created_at
             FROM notifications
             WHERE recipient_org_id = $1 AND read_at IS NULL
             ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
    } else {
        sqlx::query_as::<_, NotificationRow>(
            "SELECT id, recipient_user_id, recipient_org_id, type, payload,
                    source_event_id, read_at, created_at
             FROM notifications
             WHERE recipient_org_id = $1
             ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
    }
    .bind(org)
    .bind(limit)
    .bind(offset)
    .fetch_all(state.pool())
    .await?;

    Ok(Json(rows.iter().map(NotificationRow::to_dto).collect()))
}

/// `GET /notifications/unread_count` — badge count for the caller's org.
///
/// # Errors
///
/// `403` for org-less principals.
pub async fn unread_count(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<Json<i64>> {
    let org = user.org_id.ok_or(ApiError::Forbidden)?;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notifications
         WHERE recipient_org_id = $1 AND read_at IS NULL",
    )
    .bind(org)
    .fetch_one(state.pool())
    .await?;
    Ok(Json(count))
}

/// `PUT /notifications/:id/read` — mark one row read; other orgs' rows are
/// 404s.
///
/// # Errors
///
/// `404` when the notification does not belong to the caller.
pub async fn mark_read(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(notification_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let org = user.org_id.ok_or(ApiError::Forbidden)?;
    let updated = sqlx::query(
        "UPDATE notifications SET read_at = now()
         WHERE id = $1 AND recipient_org_id = $2 AND read_at IS NULL",
    )
    .bind(notification_id)
    .bind(org)
    .execute(state.pool())
    .await?
    .rows_affected();
    if updated == 0 {
        // Either missing, foreign, or already read — all idempotent 404/OK
        // territory; distinguish foreign/missing as 404, already-read as OK.
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM notifications WHERE id = $1 AND recipient_org_id = $2)",
        )
        .bind(notification_id)
        .bind(org)
        .fetch_one(state.pool())
        .await?;
        if exists {
            return Ok(StatusCode::OK);
        }
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::OK)
}

/// `PUT /notifications/read-all` — clear the caller's org badge.
///
/// # Errors
///
/// `403` for org-less principals.
pub async fn mark_all_read(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<StatusCode> {
    let org = user.org_id.ok_or(ApiError::Forbidden)?;
    sqlx::query(
        "UPDATE notifications SET read_at = now()
         WHERE recipient_org_id = $1 AND read_at IS NULL",
    )
    .bind(org)
    .execute(state.pool())
    .await?;
    Ok(StatusCode::OK)
}

/// `GET /notifications/stream` — live inbox updates over Server-Sent
/// Events; EventSource clients authenticate via `?access_token=` (the
/// platform middleware accepts it). Payloads are notification DTOs.
///
/// # Errors
///
/// `500` when live wiring is absent (misconfigured deployment).
pub async fn stream(
    State(state): State<AppState>,
) -> ApiResult<Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>>> {
    let fanout = state.fanout().ok_or(ApiError::Internal(anyhow::anyhow!(
        "live updates not configured"
    )))?;
    let rx = fanout.subscribe();

    let events =
        tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|message| match message {
            Ok(payload) => Some(Ok(SseEvent::default()
                .event("notification")
                .data(payload.as_ref()))),
            // Client fell behind; it reconciles by refetching the inbox.
            Err(_lagged) => None,
        });

    Ok(Sse::new(events).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Convenience for tests and the consumer loop: build the live payload from
/// a persisted row.
#[must_use]
pub fn live_payload(row: &NotificationRow) -> serde_json::Value {
    serde_json::to_value(row.to_dto()).unwrap_or_default()
}
