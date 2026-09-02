# ADR-010: Testing strategy and coverage enforcement

**Status:** Accepted

## Context

The team opted for an extensive testing investment: the licensing state machine and
scene overlap logic are the correctness-critical heart of the product, and the
microservice topology multiplies integration risk.

## Decision

**Layers**
1. **Unit** (`#[test]` / `#[tokio::test]`): state machine transition table (exhaustive
   matrix), scene overlap math, idempotency keys, template rendering.
2. **Property-based** (`proptest`): license transition sequences never reach invalid
   states; arbitrary scene sets validated by the constraint are non-overlapping.
3. **HTTP/integration per service**: Axum routers driven via `tower::ServiceExt::oneshot`
   with real dependency containers from **testcontainers-rs** (Postgres, Redis, Kafka,
   ElasticSearch, MinIO). Covers: auth flows, overlap constraint, outbox→Kafka relay,
   consumer→index roundtrip, SSE receives event on transition, inbox dedup.
4. **End-to-end** (`Playwright`): full compose stack; happy path = login → search song →
   create offer → label counter-offer → studio accepts → live SSE update visible in two
   sessions; notification bell reflects the offer live.
5. **Frontend component tests**: Vitest + React Testing Library on role/state-gated UI
   (transition buttons, status badges, notification center).

**Quality gates (CI, GitHub Actions)**
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- Unit + integration tests on every PR (testcontainers against the runner's Docker).
- **cargo-llvm-cov** with enforced thresholds: ≥80% per crate, ≥70% overall; failures
  block merge.
- Frontend: `tsc`, ESLint, Vitest, production build.
- Compose-based Playwright e2e as a separate required job.

## Consequences

**Positive**
- Correctness-critical logic is exhaustively and property tested, not just example
  tested.
- Real dependencies in integration tests catch SQL/consumer drift early.

**Negative**
- CI wall-time grows with containers; mitigated by job parallelism and selective
  profiling later.
- Coverage thresholds can incentivize weak tests; review discipline remains necessary.
