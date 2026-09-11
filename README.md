# Music Licensing Workflow

A backend service for ACME BROS PICTURES: studios place songs into movie scenes as tracks, negotiate license fees with record labels through a stateful workflow, and both parties watch state changes arrive live over SSE. Built with Express, Knex and TypeScript - one process, no frontend.

## Running this stack

```bash
$ docker compose up --build -d      # or: podman compose up --build -d
$ curl localhost:8000/api/v1/health # {"ok":true,"redis":true}
```

The api container runs migrations and seeds before boot. Two users come seeded, password `password` for both: `grace@gotham.studios` (movie studio) and `mark@evilrecords.com` (record label).

## Local development

No infra needed - SQLite covers dev and tests, Postgres only runs in compose. `NODE_ENV` selects the knex profile (`development` default, `test`, `docker`); the same migrations run on both databases.

```bash
$ pnpm install
$ pnpm exec tsx index.ts
$ pnpm test:local    # 30 api tests, no infra required
$ pnpm test:redis    # event tests; needs redis on localhost:6379, skips if unreachable
```

## Architecture

```
                +------------------------------------------+
                |        api :8000 (Express + TS)          |
                |  REST + SSE, JWT verified per request    |
                |  claims: {user_id, company_id, user_type}|
                +------+--------------------------+--------+
                       |                          |
              +--------v--------+        +--------v--------+
              | Postgres        |        | Redis           |
              | movies, scenes, |        | pub/sub channels|
              | songs, licenses |        | events:{org_id} |
              | refresh_tokens  |        |                 |
              +-----------------+        +-----------------+
```

Everything is scoped by the `company_id` claim: studios only ever see their own movies, scenes and licenses, labels their own songs. Where a party should not learn that a resource exists at all, the api answers 404 rather than 403 - no existence leaks.

## The license state machine

A "track" is a `licenses` row - a song placed on a scene with start offset and length, a fee, and a state:

```
   studio opens (fee required)        label counters            studio re-offers
  ─────────────────────► OFFER ◄──── COUNTER ────────────► OFFER  ... (loop)
                           │             │
                           └─ label ACCEPT/REJECT   studio ACCEPT/REJECT on COUNTER
```

The party that did not make the current offer is the one who closes it: the label closes an OFFER, the studio closes a COUNTER. Three properties hold across every transition:

* **Atomicity.** Every state change carries `UPDATE ... WHERE state = <expected>`, so two parties closing at once yields a 409 for the loser instead of a double-close.
* **Party-scoped visibility.** Non-parties get the same "does not exist" answer as everyone else.
* **Role checks at the boundary.** Wrong role or wrong state is a 403 with a message that says which one.

## Realtime

Every successful license write publishes an event to `events:{studio_id}` and `events:{label_id}` on Redis. `GET /api/v1/licenses/events` subscribes to the caller's company channel and streams server-sent events:

```text
event: connected
event: license
data: {"type":"created","license_id":"...","to_state":"OFFER","license_fee":4242}
```

The failure mode is deliberate: if Redis is unreachable the endpoint 503s immediately instead of half-opening a stream, and publishing never fails the mutation - the database row is the source of truth, events are best effort. A mid-stream Redis failure ends the stream, and `EventSource` reconnect semantics handle the retry. `/health` reports `"redis": true|false` so it's visible at a glance.

## Decisions and tradeoffs

* **REST over GraphQL.** The access patterns (movies for a studio, licenses for a scene, events for a company) are known up front, so there was no case for client-shaped queries - and REST's status code semantics carry real weight in a negotiation API where 403 (wrong role) and 404 (not your license) mean different things.
* **Express + Knex + TypeScript.** Boring on purpose. Typed table definitions in `db.ts`, SQL-first queries, controllers as plain `Result`-returning functions so routes stay declarative.
* **Dual database profile.** SQLite for dev and tests keeps the suite fast and infra-free; Postgres matches the production requirement. Same migrations, profile selected by `NODE_ENV`.
* **JWT (1h) + refresh tokens (7d) in Postgres.** Claims `{user_id, company_id, user_type}` carry everything the business logic needs - no user lookups on hot paths. Known tradeoff: refresh tokens are stored plain and don't rotate on use; revocation is a row delete, which is at least simple to reason about. Hashing at rest and single-use rotation is the obvious next step.
* **SSE over WebSockets.** License state changes flow in one direction, so a duplex channel buys nothing. Redis pubsub keeps the design correct across multiple api instances.
* **scrypt** (node builtin) for password hashing - constant-time compare, no native dependencies.

## Tests and CI

30 integration tests (node:test + supertest) covering the auth flow, company scoping, scene numbering, and the full negotiation lifecycle including race and role-negative cases. Realtime has its own suite: pubsub delivery to both company channels, and SSE framing verified over a real HTTP stream - these skip when Redis is unreachable locally. CI (GitHub Actions) runs typecheck plus both suites, with Redis as a service container.

## Assumptions

* A "track" is the license row itself; songs can be reused across scenes and movies.
* Fees are required when opening an offer and when countering - there is no fee-less "interested" gesture.
* Seeded accounts exist for convenience; there is no signup flow.
* No frontend by scope; the api is the deliverable.
