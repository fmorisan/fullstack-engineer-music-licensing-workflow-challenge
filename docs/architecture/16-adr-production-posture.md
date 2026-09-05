# ADR-016: Production deployment posture

**Status:** Accepted (topology); implementation pending — deliberately deferred
until the prod overlay is requested. The dev/minikube deployment of ADR-013
remains the implemented truth.

## Context

ADR-013's Kubernetes deployment is deliberately dev-posture: side-loaded
images, localhost port-forward parity, dev credentials, public-read buckets,
single-node stateful everything. The application architecture, by contrast,
is already production-shaped — stateless services, Redis-fanned SSE,
`FOR UPDATE SKIP LOCKED` outbox relaying, consumer groups with self-healing
zombie guards — so the gaps live entirely in the deployment and data layers.

The repo is a monorepo, and that is intentional: the domain's center of
gravity is `licensing-core` (state machine, roles, two pinned Kafka
contracts) whose changes are cross-service by nature. A single workspace
gate keeps contracts honest in a way per-repo CI cannot. The deployment
independence a service-per-repo shape gives for free is therefore
reconstructed deliberately on top — per-service images, digest-scoped
rollouts, a dedicated deploy repo — rather than given up.

## Decision

| Layer | Choice |
|---|---|
| **Media** | AWS S3 + CloudFront — pre-signed PUTs straight to S3 (CORS scoped to the app origin); reads served by CloudFront over a **private bucket via OAC**. Closes the dev public-read posture ADR-009 anticipated. `VITE_MEDIA_URL` points at the distribution; `S3_ENDPOINT` names real S3, so pre-signed hosts are browser-reachable by construction and port-forward parity dies. |
| **Postgres** | **One managed RDS instance** — per-service logical databases and per-service users; ADR-003's seam survives, and a later split is mechanical (repoint each `DATABASE_URL`, move its database). Backups/PITR/failover become the platform's; migrations run as pre-deploy Jobs. |
| **Kafka** | **Self-hosted via Strimzi** — three brokers, RF=3, `min.insync.replicas=2`, KRaft. Consumer zombie-guards plus idempotent at-least-once consumers tolerate broker churn, and full topic retention keeps both consumers replay-rebuildable from history. |
| **Redis** | **Self-hosted** — primary + replica with AUTH + TLS on PVCs. Degradation is pacing, not correctness: search cache misses fall through to ElasticSearch; PubSub loss delays live updates while clients reconcile by refetch. |
| **ElasticSearch** | **Self-hosted via ECK** — three nodes across AZs, security enabled (TLS + auth — the hard requirement the dev stack skips), S3 snapshots. Fully rebuildable from `song.events` by design, which is what makes self-hosting acceptable. |
| **Secrets** | **SealedSecrets, first pass** — simplest GitOps-pure option: encrypted secrets committed to the deploy repo, no cloud IAM coupling; covers DB credentials, S3 keys, and the real JWT keypair. Cost: re-sealing on rotation, cluster-key scoping. The upgrade path to External Secrets + AWS Secrets Manager is non-breaking — workloads consume plain Secrets either way. |
| **Images** | **One build, per-service tags in prod** — the single cargo graph and cache-mount pipeline of ADR-014 are kept; the builder then emits six per-service images (each a one-binary copy onto the shared base) plus the shared dev image. This exercises ADR-014's recorded escape hatch: deployment independence is recovered without paying back any build-time win. |
| **GitOps** | **A dedicated deploy repo owns production.** Details below. |

### The deploy repo and the delivery loop

The app repo keeps the kustomize base and the dev/minikube deployment
(ADR-013). The deploy repo holds the overlays (per-service digest pins,
replicas, ingress, SealedSecrets), the ArgoCD app-of-apps Application, and
a bump job. Overlays consume the base as a **kustomize remote reference
pinned to a ref** — never a vendored copy (drift by construction).

1. A green build in the app repo — the whole workspace gate, which is what
   keeps contracts honest — pushes six per-service images and calls the
   deploy repo's bump job through an authenticated webhook (secret held
   only by the upstream repo's CI; forks cannot trigger deploys).
2. The bump job lands **one provenance commit**: base ref and six image
   digests, all derived from the same green SHA. They cannot skew, and the
   commit is the deployment record.
3. **Staging syncs automatically; production stays a human action** — a
   merge in the deploy repo promoting the pins. The manual gate is a
   deliberate feature; it is auditable Git instead of tribal knowledge.
4. ArgoCD syncs Git only — no Image Updater write-back, no mutable tags in
   prod (both considered and rejected: drift and auditability). A registry
   webhook pings ArgoCD's refresh endpoint for immediacy, nothing more.
5. Sync waves: SealedSecrets resolve → stateful layer (Strimzi/ECK/Redis —
   unchanged specs roll nothing) → **PreSync Jobs** (RDS database/user
   bootstrap, per-service migrations, gated on success) → the six service
   Deployments → frontend and ingress.
6. **Scoping is emergent from content addressing**: unchanged binaries
   produce byte-identical images, identical digests, unchanged pod
   templates — only changed services roll. A `licensing-core` contract
   change bumps all six and rolls everything, coordinated by construction,
   migrations first.

Rollback is `git revert` in the deploy repo: one commit restored, ArgoCD
syncs backward through the same waves. Schema migrations are the one
irreversible direction, which is why they are PreSync Jobs reviewed in the
promotion PR, not deployment surprises.

## Consequences

- The prod overlay **shrinks**: no postgres/minio/minio-init manifests; a
  bootstrap Job creates the five databases and users on RDS; Strimzi and
  ECK replace the hand-rolled single-node StatefulSets.
- Dev/prod divergence becomes explicit and documented rather than
  accidental: bucket privacy, real pre-sign endpoints, operator-managed
  stateful layers, per-service images.
- Self-hosting Kafka, ElasticSearch, and Redis means owning their upgrades
  and snapshots — a bounded obligation, since each is either
  replay-rebuildable (Kafka topics, ES index) or transient (Redis).
- **No application code changes are required**: pre-signing is
  endpoint-agnostic (proven by the `minio:9000` → `localhost:9000` move),
  consumers already survive rebalances, and `DATABASE_URL` is the only
  Postgres coupling.
- Known monorepo costs are accepted with eyes open: a flaky suite can
  freeze the whole gate (mitigable with path-filtered jobs later), and
  deployment independence is maintained by the image-split machinery
  rather than by repo shape. The seams for extracting any service into
  its own repo later are preserved by ADR-001's boundaries — the split is
  mechanical when a service needs its own release cadence or owners.
