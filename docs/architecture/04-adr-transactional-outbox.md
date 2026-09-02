# ADR-004: Transactional outbox for change capture (vs. Debezium)

**Status:** Accepted

## Context

The design doc calls for "change data capture via Kafka" to hydrate the ElasticSearch
song catalog. The two realistic implementations are log-based CDC (Debezium + Kafka
Connect against Postgres WAL) or an application-level transactional outbox.

## Decision

- **Transactional outbox** in the writing service:
  1. Domain mutation and an `outbox` row are committed in the **same Postgres
     transaction**.
  2. A relay task (`crates/platform`) polls/delivers due outbox rows to Kafka
     (`song.created`, `song.updated`, `song.deleted`, `license.updated`), marking rows
     sent in batches. At-least-once delivery.
- Consumers are **idempotent**: the search indexer upserts/deletes by song id;
  notification_service deduplicates on an idempotency key (see ADR-006).
- Debezium remains documented as the production alternative: it would remove the
  application-level relay and capture schema changes generically, at the cost of running
  Kafka Connect, configuring logical replication (`wal_level=logical`), and operating
  connector lifecycle.

## Consequences

**Positive**
- No Kafka Connect/Debezium containers to operate, debug, or keep compatible in CI.
- The pattern is unit/integration-testable with ordinary Postgres testcontainers.
- One shared relay implementation (`crates/platform`) reused by song_service and
  license_service.

**Negative**
- At-least-once (not exactly-once) semantics; duplicates must be handled downstream —
  they are, by design.
- The outbox table and relay are code we own; Debezium would externalize this.
- Relay polling adds small latency (sub-second with `FOR UPDATE SKIP LOCKED` polling)
  versus near-instant WAL tailing.
