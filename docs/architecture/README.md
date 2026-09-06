# Architecture Documentation

## Documents

| # | Document | Subject |
|---|---|---|
| 00 | [Design](./00-design.md) | Original product design, domain model, and target architecture |
| 01 | [ADR-001](./01-adr-rust-axum-microservices.md) | Rust + Axum microservices on a Cargo workspace |
| 02 | [ADR-002](./02-adr-traefik-forward-auth-gateway.md) | Traefik + forward-auth (superseded by ADR-011) |
| 03 | [ADR-003](./03-adr-sqlx-db-per-service.md) | SQLx as the data layer; database-per-service |
| 04 | [ADR-004](./04-adr-transactional-outbox.md) | Transactional outbox for change capture (vs. Debezium) |
| 05 | [ADR-005](./05-adr-sse-redis-pubsub.md) | SSE over Redis PubSub for real-time updates |
| 06 | [ADR-006](./06-adr-notification-service.md) | Dedicated notification service with channel abstraction |
| 07 | [ADR-007](./07-adr-rest-only.md) | REST-only API surface (no GraphQL) |
| 08 | [ADR-008](./08-adr-frontend-vite-react.md) | Vite + React SPA frontend |
| 09 | [ADR-009](./09-adr-ids-and-media.md) | UUIDv7 identifiers and MinIO pre-signed media uploads |
| 10 | [ADR-010](./10-adr-testing-strategy.md) | Testing strategy and coverage enforcement |
| 11 | [ADR-011](./11-adr-asymmetric-jwts-kong-edge.md) | RS256/JWKS tokens, Kong edge, dual validation |
| 12 | [ADR-012](./12-adr-token-lifecycle-refresh-sessions.md) | 15-minute access tokens + rotating refresh sessions |
| 13 | [ADR-013](./13-adr-kubernetes-minikube.md) | Kubernetes (minikube) deployment with port-forward parity |
| 14 | [ADR-014](./14-adr-shared-image-cache-mounts.md) | One shared service image, incremental cache-mount builds |
| 15 | [ADR-015](./15-adr-observability-grafana.md) | Prometheus metrics + Grafana dashboard |
| 16 | [ADR-016](./16-adr-production-posture.md) | Production posture: S3/CloudFront, RDS, self-hosted statefuls, deploy-repo GitOps |

## System Context

```
                          ┌───────────────────────────────┐
                          │  Kong (db-less L7 gateway)    │
                          │  RS256 jwt plugin at the edge │
                          │  + CORS (preflight-safe)      │
                          └───────────────┬───────────────┘
            ┌──────────┬──────────┬───────┴────┬──────────┬──────────┐
            ▼          ▼          ▼            ▼          ▼          ▼
       auth_svc    movie_svc   song_svc   search_svc  license_svc  notification_svc
            │          │          │            │          │              │
            │  (services verify the same RS256 public key via platform::auth
            │   — dual validation, zero per-request hops; ADR-011)
            │
            ├─ Postgres (db per service: auth/movie/song/license/notification)
            ├─ RS256 keys + /.well-known/jwks.json (issuer contract)
            └─ refresh sessions: hashed, rotating, family-revocable (ADR-012)

            movie/song/license write outbox rows ──▶ Kafka (song.events,
            license.events) ──▶ search_svc consumer ──▶ ElasticSearch (+ Redis
            cache) and notification_svc (inbox in PG, Redis PubSub → SSE,
            EmailChannel → Mailpit)

            Redis PubSub also fans out license updates → /licenses/stream SSE

          MinIO (S3) <- pre-signed uploads: posters, scene captures, box art,
          audio previews
```

## Repository Layout

```
/
├── Cargo.toml              # workspace: crates/* + services/*
├── rust-toolchain.toml
├── .sqlx/                  # committed offline query-check metadata
├── Makefile                 # root orchestration (no JS tooling at root)
├── crates/
│   ├── licensing-core/     # pure domain: state machine, roles, event schemas
│   ├── platform/           # infra helpers: jwt auth middleware, outbox, kafka, redis, sse
│   └── test-support/       # testcontainers helpers, key fixtures
├── services/
│   ├── auth_service/
│   ├── movie_service/
│   ├── song_service/
│   ├── search_service/
│   ├── license_service/
│   └── notification_service/
├── frontend/               # Vite + React + TS — the only JS/TS in this repo
├── infrastructure/
│   ├── docker/             # compose stack, kong/, keys/ (dev fixtures), seed/
│   ├── k8s/                # stretch goal
│   └── pulumi/             # documentation only
├── docs/architecture/
└── .github/workflows/
```
