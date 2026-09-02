# ADR-003: SQLx as the data layer; database-per-service

**Status:** Accepted

## Context

Rust offers SQLx, SeaORM, and Diesel for Postgres access. The original plan assumed
Prisma (TypeScript-only), which is unavailable. We need migrations, async I/O for Axum,
and strong correctness guarantees.

## Decision

- **SQLx** (postgres, tokio runtime, `migrate` feature) for all Postgres access.
  - Queries are written in SQL files checked by `sqlx::query!` / `query_as!` macros at
    compile time against a live schema (via `SQLX_OFFLINE=true` + checked-in
    `.sqlx/` metadata in CI).
  - Migrations run per-service via `sqlx migrate` on service startup or via the seed job.
- **Database-per-service**: a single Postgres instance for local development, with one
  logical database per service (`auth_db`, `movie_db`, `song_db`, `search_db` not needed,
  `license_db`, `notification_db`). No service reads another service's tables;
  cross-service data is fetched over HTTP with the caller's JWT or via events.
- Domain invariants that must survive concurrency are enforced in the database — e.g.
  scene temporal overlap is prevented by an `EXCLUDE USING gist` constraint with
  `btree_gist` (`int4range(start, "end") &&`), not just application checks.

## Consequences

**Positive**
- Compile-time checked SQL eliminates a class of runtime errors ORM stringly-typed
  APIs allow.
- DB-per-service keeps schemas independently evolvable and mirrors the target
  production topology (separate instances).
- Raw SQL keeps constraint logic (exclusion constraints, CTEs for outbox) expressible.

**Negative**
- More hand-written SQL than an ORM; entity mapping boilerplate is on us.
- CI needs `SQLX_OFFLINE` metadata discipline: developers must re-run
  `cargo sqlx prepare` after touching queries.
