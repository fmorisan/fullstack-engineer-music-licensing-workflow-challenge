# ADR-005: SSE over Redis PubSub for real-time updates

**Status:** Accepted

## Context

The product requires that users "immediately see updates" in licensing status. Options
considered: WebSockets, GraphQL subscriptions, and Server-Sent Events. The design doc
commits to SSE with Redis PubSub as the fan-out transport.

## Decision

- **SSE** (`axum::response::sse::Sse`) with two independent streams:
  - `GET /licenses/stream` (license_service) — live license state changes.
  - `GET /notifications/stream` (notification_service) — live inbox updates.
- Fan-out via **Redis PubSub**: services publish JSON events to a channel; each service's
  SSE handler task subscribes and forwards to connected clients through an in-process
  `tokio::sync::broadcast` channel. Redis decouples publisher and streamer so multiple
  service replicas work without sticky sessions (events are broadcast to all replicas).
- Events carry enough context (ids, state, fee) for the frontend to reconcile; the
  frontend also refetches on `visibilitychange` to cover gaps, since PubSub is
  fire-and-forget.
- Traefik streams SSE by default; compression middlewares are excluded on the SSE routes.

## Consequences

**Positive**
- SSE is unidirectional, HTTP-native: trivially proxied, no upgrade handshake, native
  `EventSource` browser API, automatic reconnection built in.
- Redis PubSub makes fan-out a solved problem and adds a bonus-tier technology with
  real purpose.
- Two decoupled streams keep services independent.

**Negative**
- Fire-and-forget transport: missed events while disconnected are not replayed from
  Redis (the persisted inbox + refetch-on-focus compensates for notifications; license
  views refetch on stream (re)connect).
- One SSE connection per feed per open tab; acceptable (browsers allow ~6 per host).
- Server push only; if bidirectional needs emerge, WebSockets would replace this.
