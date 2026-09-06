# AGENTS.md — Project Conventions

## Commit conventions (mandatory)

- **Conventional Commits** style: `type(scope): imperative summary` (lowercase, no period).
- **Service-feature oriented commits only** — never lump an entire phase into one commit.
  One logical feature per service per commit; small is good.
- Scopes in use:
  - Services: `auth-service`, `movie-service`, `song-service`, `search-service`,
    `license-service`, `notification-service`
  - Crates: `licensing-core`, `platform`, `test-support`
  - Others: `frontend`, `architecture` (docs/ADRs), `infra` (docker/compose/traefik), `ci`
- Examples:
  - `feat(auth-service): implement /verify for forward-auth`
  - `feat(licensing-core): add license state machine transition table`
  - `fix(movie-service): reject overlapping scene intervals atomically`
  - `chore(workspace): ...`, `docs(architecture): ...`, `ci: ...`
- Every commit must keep the workspace buildable (unit tests + `cargo check` green at
  each step) so the history stays bisectable. The root `Cargo.toml` `members` list grows
  as crates/services land — include the member line addition in the commit that
  introduces the crate.

## Build & test commands

- `make check` — full local gate (fmt-check, clippy with `-D warnings`, tests, build)
- `make <service>` — run one service locally (auth 8101 … notification 8106)
- `make frontend-dev` / `frontend-build` / `frontend-lint` — frontend (only JS/TS in repo)
- `make up` / `make down` — local compose stack (Phase 1+)

## Hard constraints

- **Rust + Axum only** for all backend services. No JS/TS outside `frontend/`.
- No root `package.json`; cross-stack orchestration goes through the `Makefile`.
- New architectural decisions get an ADR in `docs/architecture/`.
- Domain rules (state machine, roles) live in `crates/licensing-core` exactly once;
  services must not re-implement them.
- Concurrency-sensitive invariants are enforced at the database level (e.g. scene
  overlap via EXCLUDE constraint), not only in application code.

## Architecture reference

See `docs/architecture/README.md` for the system diagram, repo layout, and the ADR index.
