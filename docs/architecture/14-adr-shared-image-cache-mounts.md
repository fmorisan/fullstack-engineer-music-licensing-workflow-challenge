# ADR-014: One shared service image with incremental cache-mount builds

**Status:** Accepted

## Context

Each of the six services had its own image built from a shared, parameterized
Dockerfile (`ARG SERVICE`). The cost of that shape grew with the project:

- Any source change recompiled `licensing-core`, `platform`, and the target
  binary **six times** — once per image — because the per-build `target/`
  never survived the layer boundary.
- cargo-chef cached *dependencies* as a layer, but any dependency change
  re-cooked the entire tree, and its headline benefit (sharing the dep layer
  across the six builds) only materialized when the layer cache held.
- `aws-lc-sys` — pulled by the AWS SDK's default rustls crypto provider —
  was the slowest, most memory-hungry crate in the graph and the recurring
  OOM source on the constrained build VM. It is also runtime-dead weight:
  services only ever *pre-sign* S3 URLs, which is offline SigV4 math; the
  SDK's HTTP client is dialed exclusively by test fixtures (bucket setup).

## Decision

**One build, one image, six commands.** `cargo build --release` with
explicit `-p <service>` selection compiles all six binaries in a single
dependency graph — shared crates build exactly once. The runtime image
(`acme-services:dev`) carries all six binaries; compose and Kubernetes
select the service via `command:`.

**BuildKit cache mounts instead of cargo-chef.** `RUN --mount=type=cache`
pins the cargo registry and `target/` across builds, so a rebuild
recompiles only the crates whose sources changed — verified: a one-service
change rebuilds exactly that binary. Artifacts are `cp`'d out of the mount
within the same `RUN` (cache-mount contents do not persist in layers).
chef is dropped: workspace-wide selection made its sharing redundant, and
its layer-granularity (all deps on any manifest change) is coarser than
cargo's own fingerprints.

**`aws-lc-sys` leaves release images.** platform gates the SDK's https
stack behind a `sdk-https` feature (default off at the workspace root;
test-support opts back in for the MinIO bucket fixtures). Images build with
`--no-default-features`; tests keep the SDK client working through
test-support's dev-dependency edge. One subtlety, verified by compiled
artifacts rather than `cargo tree` (whose feature views unify the whole
workspace and mislead): selection must be the explicit `-p` list —
`--bins` builds test-support's lib as a workspace member, whose
`sdk-https` request unifies the feature back on for everyone.

## Consequences

- Image builds drop from six cargo graphs to one; dep changes rebuild
  incrementally; `aws-lc-sys` — the OOM source — is gone from release
  builds entirely (63 crates total, none of them aws-lc).
- One 182 MB image replaces six ~124 MB ones; minikube side-loads a single
  tar. Per-service isolation at the image layer is traded away — a dev
  posture choice; production wanting per-service images can split the same
  builder at the copy step.
- Cache mounts live in the builder's cache store: aggressive prunes
  (`podman build prune`) clear them, costing one cold build — the same
  price chef's layer cache paid under image prunes.
- rdkafka stays bundled-static (cmake): Debian bookworm's librdkafka is
  far too old for `rdkafka-sys` 4.10, and a trixie base-chase buys ~2
  minutes at real churn cost.
