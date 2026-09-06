# ADR-015: Observability — Prometheus metrics and a Grafana dashboard

**Status:** Accepted

## Context

The stack's only introspection was per-service `/healthz`. Debugging the
consumer-zombie incidents meant reading Kafka tooling output and logs by
hand; there was no way to see request rates, latencies, event flow, or
live SSE connections as they happened. The Kubernetes deployment (ADR-013)
made a metrics deployment natural to ship alongside.

## Decision

**Services export Prometheus metrics from `platform`.** A single,
lazily-installed recorder per process; an axum middleware records
`http_requests_total{method,route,status}` and
`http_request_duration_seconds{method,route}` histograms. The middleware
layers *outside* auth (401s are signal) and the `/metrics` route is added
*after* the layer, so scrapes never pollute the series. Pipeline counters
instrument the places events actually move:

- `events_consumed_total{group}` — Kafka messages received (platform's
  `EventConsumer`)
- `outbox_published_total` — outbox rows relayed (platform's `OutboxRelay`)
- `consumer_rebuilds_total{reason}` — zombie-guard supervisor rebuilds
- `sse_connected{stream}` — live SSE clients, via an RAII guard that rides
  the stream and decrements on disconnect

**Kong exposes edge metrics** through the prometheus plugin on its status
listener — per-route traffic and latency at the gateway, complementing the
services' own series.

**The monitoring stack deploys with the kustomize base** (ADR-013):
Prometheus with static scrape configs (six services, Kong, and three
lightweight exporters — redis, postgres, elasticsearch; Kafka and MinIO are
covered by the application-level consumer/outbox/HTTP series rather than
their own exporters), and Grafana with provisioned datasource and an
"ACME Licensing — System" dashboard: service up/down, request rate and p95
latency by service, 5xx rates, Kafka event flow and outbox publish rate,
consumer rebuilds, live SSE clients, gateway traffic by route, and
datastore panels. `make k8s-up` port-forwards Grafana on :3000 with
anonymous admin access.

## Consequences

- "Is the system healthy" has an answer at a glance, and incident classes
  like the consumer zombie appear as a `consumer_rebuilds_total` tick
  instead of a silent stall.
- Static scrape configs (no service monitors, no Prometheus operator):
  right-sized for a dev cluster; a real environment would use SD.
- Traces (OpenTelemetry) are deliberately out of scope — metrics carry the
  system-state story; correlating one offer across services is a possible
  follow-up.
- The metrics endpoints ride the existing routers: no extra ports,
  probes, or sidecars, in compose mode or k8s alike.
