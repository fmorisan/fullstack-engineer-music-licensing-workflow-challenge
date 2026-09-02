# Architecture Documentation

## Documents

| # | Document | Subject |
|---|---|---|
| 00 | [Design](./00-design.md) | Original product design, domain model, and target architecture |
| 01 | [ADR-001](./01-adr-rust-axum-microservices.md) | Rust + Axum microservices on a Cargo workspace |
| 02 | [ADR-002](./02-adr-traefik-forward-auth-gateway.md) | Traefik + forward-auth in place of a dedicated api_gateway service |
| 03 | [ADR-003](./03-adr-sqlx-db-per-service.md) | SQLx as the data layer; database-per-service |
| 04 | [ADR-004](./04-adr-transactional-outbox.md) | Transactional outbox for change capture (vs. Debezium) |
| 05 | [ADR-005](./05-adr-sse-redis-pubsub.md) | SSE over Redis PubSub for real-time updates |
| 06 | [ADR-006](./06-adr-notification-service.md) | Dedicated notification service with channel abstraction |
| 07 | [ADR-007](./07-adr-rest-only.md) | REST-only API surface (no GraphQL) |
| 08 | [ADR-008](./08-adr-frontend-vite-react.md) | Vite + React SPA frontend |
| 09 | [ADR-009](./09-adr-ids-and-media.md) | UUIDv7 identifiers and MinIO pre-signed media uploads |
| 10 | [ADR-010](./10-adr-testing-strategy.md) | Testing strategy and coverage enforcement |

## System Context

```
                        ┌──────────────────────────┐
                        │  Traefik (L7 gateway)    │
                        │  forward-auth -> /verify │
                        └───────────┬──────────────┘
            ┌──────────┬────────────┼─────────┬──────────┐
            ▼          ▼            ▼         ▼          ▼
       auth_svc    movie_svc    song_svc  search_svc  license_svc   notification_svc
            │          │            │         │          │               │
            └──── Postgres (db per service) ──┴── outbox ┘    Kafka: license.updated
                               │                                │
                               │                        notification_svc
                      Kafka: song.* (outbox)                  │
                               │                      ┌────────┴────────┐
                               ▼                      │  inbox (PG)     │
                        search_svc consumer           │  Redis -> SSE   │
                               │                      │  EmailChannel   │
                        ElasticSearch                 │  (Mailpit)      │
                          + Redis cache               └─────────────────┘
                               │
            Redis PubSub -> SSE /licenses/stream (license_svc)

          MinIO (S3) <- pre-signed uploads: posters, scene captures, box art, audio previews
```

## Repository Layout

```
/
├── Cargo.toml              # workspace: crates/* + services/*
├── rust-toolchain.toml
├── justfile                # root orchestration (no JS tooling at root)
├── crates/
│   ├── licensing-core/     # pure domain: state machine, roles, event schemas
│   ├── platform/           # infra helpers: outbox relay, kafka, redis pubsub, sse
│   └── test-support/       # testcontainers helpers, fixtures
├── services/
│   ├── auth_service/
│   ├── movie_service/
│   ├── song_service/
│   ├── search_service/
│   ├── license_service/
│   └── notification_service/
├── frontend/               # Vite + React + TS — the only JS/TS in this repo
├── infrastructure/
│   ├── docker/             # compose stack, traefik config, seed scripts
│   ├── k8s/                # stretch goal
│   └── pulumi/             # documentation only
├── docs/architecture/
└── .github/workflows/
```
