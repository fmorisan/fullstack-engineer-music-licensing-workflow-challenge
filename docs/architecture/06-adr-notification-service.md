# ADR-006: Dedicated notification service with channel abstraction

**Status:** Accepted

## Context

Transactional emails (offer received, counter-offer, accepted, rejected) must be sent on
license transitions. The channel set is expected to grow (push notifications via
APNs/FCM), and the UI must display notifications in an inbox.

## Decision

- A dedicated **notification_service** (Rust/Axum) owns everything user-facing about
  notifications:
  - Consumes `license.updated` from **Kafka** (durable, decoupled; email is now
    event-driven rather than an inline HTTP call).
  - Persists each notification to its own Postgres database (`notification_db`):
    `id`, `recipient_user_id`, `type`, `payload` (JSONB), `read_at`, `created_at`,
    and a unique idempotency key `(source_event_id, channel)`.
  - Fans out live via **Redis PubSub → `GET /notifications/stream`** (SSE).
  - Delivers via a **channel abstraction**:
    `trait NotificationChannel { async fn deliver(&self, notification) }`
    with `EmailChannel` (Mailpit HTTP API in local dev; SMTP-capable later) shipped now
    and a `PushChannel` slot for APNs/FCM.
- REST API for the inbox: `GET /notifications`, `GET /notifications/unread_count`,
  `PUT /notifications/:id/read`, `PUT /notifications/read-all`.
- license_service contains **no** notification/email code; it only emits
  `license.updated` via the outbox (ADR-004).
- Recipient resolution: the event includes the counterparty user id; notification_service
  maps event → notification type (OFFER_RECEIVED, COUNTER_OFFER_RECEIVED, OFFER_ACCEPTED,
  OFFER_REJECTED).

## Consequences

**Positive**
- license_service stays pure domain logic.
- Kafka consumption means no lost notifications if the service is briefly down
  (consumer offsets), and delivery is retried naturally.
- New channels (push, SMS, webhooks) are additive implementations of a trait, with
  routing by notification type.
- The inbox is a first-class product surface for the UI (bell, badge, mark-as-read).

**Negative**
- One more service to build, deploy, and test.
- At-least-once Kafka semantics → deduplication via the idempotency key is mandatory.
- Channel delivery is best-effort after persistence; a failed email is visible in logs
  (retry policy documented as future work — a `deliveries` table with status).
