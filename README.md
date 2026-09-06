# ACME Licensing — Music Licensing Workflow

Full-stack implementation of the music licensing challenge for **ACME BROS
PICTURES**: Rust + Axum microservices, a React SPA, and a complete local
stack — negotiation state machine, real-time updates over SSE, pre-signed
media uploads, event-driven search, and an NLE-style timeline UI.

**Original challenge brief preserved at the bottom of this file.**

## Quickstart

Requires: a container runtime (tested with podman; Docker works too),
Rust 1.96, and Node 22. `make` orchestrates everything.

```bash
make up        # infra + 6 services + frontend, healthchecked, MinIO bootstrapped
make seed      # demo users, a movie with scenes, a label catalog (idempotent)
```

| What        | Where                                          |
| ----------- | ---------------------------------------------- |
| Frontend    | http://localhost:5173                          |
| Gateway     | http://localhost:8080 (Kong, db-less)          |
| Mailpit     | http://localhost:8025 (outbound email inbox)   |
| Kong admin  | http://localhost:8081                          |
| MinIO       | http://localhost:9001 (media objects)          |

Demo accounts (from `make seed`):

| Role    | Email                     | Password       |
| ------- | ------------------------- | -------------- |
| Studio  | `grace@acme.example`      | `nw-derulo-99` |
| Label   | `warp-label@acme.example` | `label-pass-99`|
| Admin   | `admin@acme.example`      | `admin-pass-99`|

### Kubernetes (minikube)

The same stack, same localhost URLs, on Kubernetes:

```bash
make down     # port-forwards reuse the compose ports
make k8s-up   # apply manifests, side-load images, wait for health, port-forward
make seed     # demo data through the gateway, exactly as on compose
```

`make k8s-down` removes the namespace but keeps the cluster (images stay
loaded — redeploys are fast). The Playwright e2e suite passes unchanged
against this deployment. Topology and the port-forward-parity decision:
[ADR-013](docs/architecture/13-adr-kubernetes-minikube.md).

The k8s deployment includes observability: every service exports
Prometheus metrics (HTTP per-route, Kafka consumers, outbox, live SSE
clients), and Grafana ships a provisioned **ACME Licensing — System**
dashboard at http://localhost:3000 ([ADR-015](docs/architecture/15-adr-observability-grafana.md)).
Service images build as **one shared image** with incremental cache-mount
compilation ([ADR-014](docs/architecture/14-adr-shared-image-cache-mounts.md)).
The production posture — AWS S3 + CloudFront, managed Postgres, self-hosted
Strimzi/ECK/Redis, per-service images from the single build, and a
deploy-repo GitOps loop — is decided in
[ADR-016](docs/architecture/16-adr-production-posture.md), implementation
deliberately pending.

## Five-minute tour

1. **Studio** (Grace): *Movies* → open a movie. The page opens with an
   **NLE-style timeline**: scene blocks on the video lane (capture photos as
   thumbnails), licenses on the music lane color-coded by state. Create a
   movie with a poster, or a scene with a capture, straight from the forms —
   pre-signed uploads land in MinIO and render immediately.
2. *Find music*: suggestions greet you before you type — **trending searches**
   (ranked by real query volume) and the **freshest catalog additions** with
   box art and 30-second preview players. License a song: drag the playback
   window on the scene track (existing licenses render behind it with live
   overlap warnings; rejected ones don't count).
3. **Label** (Warp): the bell moves **live over SSE** — no reload. The
   notification names the song, scene, and movie, and clicks through to the
   **movie context page**: the full timeline with *every* license on the
   movie (competitors read-only), your rows with counter / accept / reject.
4. Counter → the studio's badge moves live → accept from the notification →
   both sides converge on ACCEPTED on their timelines. Mailpit holds the
   emails for each step.
5. Incoming licenses / catalog pages cover the rest: label-side catalog
   management with media, studio-side per-scene license boards with fees in
   cents and a full negotiation log.

## Architecture

Six Axum services (Rust, one Postgres database each — no shared DBs), a
Kong db-less gateway, Kafka, Redis, ElasticSearch, MinIO, Mailpit.
Domain rules live exactly once in `crates/licensing-core` (state machine,
roles, event contracts); cross-service facts are validated over HTTP with
propagated JWTs; concurrency invariants are enforced by the database (scene
overlap via an EXCLUDE constraint).

```
frontend ── Kong (RS256 edge validation) ── auth / movie / song / search / license / notification
                     │                            │            │
                     │                     presigned PUT    labels' movie reads gated by a
                     │                     (browser→MinIO)   license-relationship check
song ──outbox──▶ Kafka `song.events`  ──▶ search indexer (ES) ──▶ suggestions/search (Redis cache)
license ──outbox──▶ Kafka `license.events` ──▶ notification service ──▶ inbox + SSE + email
SSE fanout via Redis PubSub, org-scoped server-side
```

Details and tradeoffs: [`docs/architecture/`](docs/architecture/README.md)
(system diagram, repo layout, and the ADR index — 12 ADRs covering the
gateway, auth, outbox, search, SSE, and media decisions).

## Testing

- **Domain**: exhaustive state-machine matrices and property tests in
  `licensing-core` (48-case oracle).
- **Integration**: every service is tested through its router against real
  Postgres/Kafka/Redis/ES/MinIO/Mailpit containers
  (`crates/test-support` fixtures). Upstreams are stubbed in-process and
  honor the propagated bearer.
- **Frontend**: Vitest + Testing Library (44 tests).
- **End-to-end**: one Playwright spec drives the **full negotiation in two
  authenticated browser contexts** — SSE badge movement, notification
  click-through, counter/accept, and Mailpit email assertions
  (`make e2e`, requires the running stack).
- **Coverage**: `make coverage` gates the full suite against ratcheted
  per-crate floors (overall ≥75%, currently 79.9%); `make coverage-html`
  produces the detailed report. CI runs the gate on every PR and attaches
  the report as an artifact.

Local gates: `make check` (fmt, clippy -D warnings, tests, build) and
`make e2e`. CI mirrors them.

## Key decisions (short version)

- **REST over GraphQL**: resource-shaped CRUD with role-scoped reads;
  real-time is a stream, not a query — SSE fits better than subscriptions
  here (ADR-005).
- **SSE over WebSockets**: one-directional server→client updates, automatic
  reconnection, and no socket-tier in the gateway; live frames fan out via
  Redis PubSub so any service instance serves any client.
- **Transactional outbox → Kafka**: license/song changes publish exactly
  what the transaction committed (at-least-once; consumers are idempotent
  by event id / document id).
- **Auth**: RS256 with a JWKS endpoint; Kong validates at the edge,
  services re-validate — SSE query-param tokens are exempted at the edge
  only (Kong OSS limitation) and verified service-side. 15-minute access
  tokens in memory; rotating refresh tokens in HttpOnly cookies with
  family revocation on replay (ADR-011/012).
- **Media**: pre-signed PUTs straight from the browser to MinIO (the JWT
  never touches object storage); keys recorded through the owning service.

Honest limitations: license windows are scene-relative playback positions —
there is no song-offset field, so an excerpt always starts at the song's
beginning; ElasticSearch and Kafka run single-node (dev posture); the dev
JWT keypair is a committed fixture; buckets are public-read locally
(production assumes CDN serving, per ADR-009).

## Repository layout

```
crates/licensing-core    domain: state machine, roles, events (no I/O)
crates/platform          shared infra: JWT auth, outbox relay, Kafka, pubsub, media
crates/test-support      testcontainer fixtures + RSA/JWT helpers
services/*               six Axum services (auth, movie, song, search, license, notification)
frontend/                React 19 + Vite SPA (TanStack Query, React Router) + e2e/
infrastructure/docker    compose files, Kong config, service/frontend images
docs/architecture        system docs + ADRs
scripts/                 coverage gate
Makefile                 every workflow (up, dev, test, coverage, e2e, seed…)
```

---

# Original challenge brief

The remainder of this file is the challenge as issued.

## 🚀 Fullstack Engineer Challenge – Music Licensing Workflow

Welcome to the **Fullstack Engineer Challenge!** 🎸🎬  
In this challenge, you'll help the fictional company **ACME BROS PICTURES** build a system to manage the **music licensing process** for their movies.

## 🎯 Context

Each movie scene can contain **multiple music tracks**, and each track requires licensing. The licensing process involves back-and-forth negotiations with rights holders (artists or labels), which makes tracking each license's progress essential.

Your task is to create a simple system to:

- Manage **tracks** for each movie scene.
- Associate a **song** to each track, specifying its start and end time.
- Track the **licensing status** of each song via a stateful workflow.
- Provide a way for other users to **immediately see updates** in licensing status (real-time or near real-time visibility).

## 📌 Requirements

### ⚙️ Tech Stack

> ⚡ **Must Include** - Use the following technologies, aligned with our tech stack:

- **Backend:** You can use any stack you're comfortable with, but we recommend using any of the following:
  - TypeScript + NestJS (you can use Fastify or Koa if you prefer)
  - Rust + Axum (you can use ActixWeb if you prefer)
- **API:** REST and/or GraphQL (you choose, and justify your choice if you only use one)
- **Frontend:** React (using any framework such as Next.js, Remix, or bare metal with Vite)
- **Database:** PostgreSQL (primary), MongoDB (optional if needed)
- **Containerization:** Docker (required)
- **Bonus:** Kafka, Redis, ArgoCD, Kubernetes (if you want to go further)

### 📦 Deliverables

> 📥 **Your submission must be a Pull Request that includes:**

- A **backend** exposing the required APIs.
- A **data model** to manage:
  - Movies, scenes, tracks, songs, and their licensing states.
- Endpoints or queries/mutations to:
  - Create a track and associate a song.
  - Update the licensing state of a track.
  - Query all tracks for a given scene/movie, including licensing status.
- A **frontend built with React** to:
  - Visualize the movie scenes and associated tracks.
  - Show licensing status.
  - Allow status updates (basic UI).
- Suggest a real-time implementation using WebSockets, GraphQL Subscriptions, or Server-Sent Events.
- Docker setup to run the entire app locally.
- A `README.md` with:
  - Setup instructions
  - Tech decisions and tradeoffs
  - If applicable, your reasoning for using REST, GraphQL, or both

> [!TIP]
> Use the `docs` folder to store any additional documentation or diagrams that help explain your solution.  
> Mention any assumptions or constraints in your `README.md`.

### 📂 Folder Suggestions

You can organize your project like this (suggested but not mandatory):

```txt
/
├── .github/
│   ├── workflows/
│   └── PULL_REQUEST_TEMPLATE.md
├── docs/
├── backend/
│   ├── src/
│   ├── test/
│   └── Dockerfile
├── frontend/
│   ├── src/
│   ├── public/
│   └── Dockerfile
├── compose.yml
├── .env.example
├── README.md
├── .prettierrc.js
├── eslint.config.mjs
└── . . .
```

## 🌟 Nice to Have

> 💡 **Bonus Points For:**

- Automated testing and CI pipeline using GitHub Actions.
- Unit or integration tests for API or key logic.
- Use of MongoDB for unstructured metadata (if justified).
- Real-time suggestion implemented (e.g., via GraphQL subscriptions or WebSockets).
- Basic usage of **Kafka or Redis** (e.g., async event messaging).
- Usage of ArgoCD or Kubernetes (not expected, but definitely cool).

> [!TIP]
> Looking for inspiration or additional ideas to earn extra points? Check out our **[Awesome NaNLABS repository](https://github.com/nanlabs/awesome-nan)** for reference projects and best practices! 🚀

## 🧪 Submission Guidelines

> 📌 **Follow these steps to submit your solution:**

1. **Fork this repository.**
2. **Create a feature branch** for your implementation.
3. **Commit your changes** with meaningful commit messages.
4. **Open a Pull Request** following the provided template.
5. **Our team will review** and provide feedback.

## ✅ Evaluation Criteria

> 🔍 **What we'll be looking at:**

- Ability to **work across the stack** (NestJS, PostgreSQL, React/Next.js/. . .).
- Clean, modular and maintainable code with proper Git usage.
- A good understanding of **data modeling and workflow management**.
- Clear written communication in your README.
- Ability to **propose real-time solutions**, even if not implemented.

## 💬 Final Notes

> [!TIP]
> This challenge is designed to be flexible!

Here are some tips to help you succeed:

- If you feel confident on the backend but less on the frontend, focus there—but try to show some basic UI.
- Likewise, if you're stronger on the frontend, make sure your backend has clean structure and endpoints.
- Time-box it: we don't expect perfection. We want to see **how you think and solve problems**.

## 🏁 Good luck and have fun building

If you have any questions, feel free to reach out.
