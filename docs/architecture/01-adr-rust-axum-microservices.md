# ADR-001: Rust + Axum microservices on a Cargo workspace

**Status:** Accepted

## Context

The challenge README offers TypeScript + NestJS or Rust + Axum/ActixWeb. The team mandated
Rust + Axum for all services, with JavaScript/TypeScript confined exclusively to the React
frontend. The design doc (00-design.md) describes five services plus an API gateway.

## Decision

- All backend services are written in **Rust** using **Axum** (with tower/tower-http
  middleware), on **tokio**.
- Services live as binary crates in a single **Cargo workspace** at the repository root,
  sharing one `target/` directory to keep compile times and disk usage sane.
- Shared code is factored into library crates:
  - `crates/licensing-core` — pure domain logic (license state machine, roles, event
    schemas, topic constants, UUIDv7 helpers). No I/O dependencies.
  - `crates/platform` — infrastructure adapters (outbox relay, Kafka producer/consumer
    wrappers, Redis PubSub, SSE helpers, tracing/config bootstrap).
  - `crates/test-support` — testcontainers helpers and fixtures.
- Six services are implemented: `auth_service`, `movie_service`, `song_service`,
  `search_service`, `license_service`, `notification_service` (see ADR-006 for the
  notification split).
- Core crate choices: `sqlx` (ADR-003), `rdkafka` (Kafka), `redis`, `aws-sdk-s3` (MinIO),
  `jsonwebtoken`, `argon2`, `reqwest`, `serde`, `tracing`, `thiserror`/`anyhow`.

## Consequences

**Positive**
- One language and one build system for the entire backend; workspace dependency
  centralization prevents version drift between services.
- Compile-time guarantees (types, checked queries via SQLx) reduce integration surprises.
- Shared crates let every service implement the outbox/relay/SSE patterns identically.

**Negative**
- Compile times are higher than TypeScript; mitigated by workspace-level target sharing,
  `cargo-chef` Docker layer caching, and optional `sccache` in CI.
- `rdkafka` links C code (`librdkafka`); we use Debian-based images with `cmake-build`
  and avoid musl/Alpine entirely.
- Smaller hiring pool and ecosystem compared to the NestJS alternative; acceptable for
  this project.
