# Shared builder for every Rust service — ONE image, six binaries.
#
#   podman build -f infrastructure/docker/services.Dockerfile -t acme-services:dev .
#
# Design (ADR-014):
# - `cargo build --release --bins` compiles all six services in a single
#   dependency graph: licensing-core, platform, and every external crate
#   are built exactly once, and any binary-specific change recompiles only
#   that binary.
# - BuildKit cache mounts carry `target/` and the cargo registry across
#   builds, so a rebuild after a source change recompiles only the crates
#   that changed — sub-minute edits instead of full cooks. This replaces
#   cargo-chef, whose layer-granular caching recompiled all dependencies on
#   any manifest change and shared its value only across the (now gone)
#   per-service builds.
# - Explicit `-p <services>` selection with `--no-default-features`
#   compiles platform without the SDK's https stack, keeping aws-lc-sys
#   (the OOM-prone C/ASM build) out of release images entirely: binaries
#   only ever pre-sign, which is offline SigV4 math. Tests re-enable it
#   through test-support's dev-dependency edge. (`--bins` would not do:
#   workspace-wide selection builds test-support's lib too, and its
#   sdk-https request unifies the feature back ON for every service.)
# - Artifacts inside a cache mount do not persist in the layer, so the
#   binaries are copied out to /out within the same RUN.

FROM rust:1.96-bookworm AS builder
# cmake + g++ for rdkafka's bundled static librdkafka and ring's C; no
# openssl (everything is rustls), no pkg-config.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake g++ \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
# Cap parallelism: unbounded rustc jobs OOM smaller build VMs.
ARG BUILD_JOBS=3
ENV CARGO_BUILD_JOBS=${BUILD_JOBS}
RUN --mount=type=cache,id=acme-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=acme-target,target=/app/target,sharing=locked \
    cargo build --release --no-default-features \
        -p auth_service -p movie_service -p song_service \
        -p search_service -p license_service -p notification_service \
    && mkdir -p /out \
    && cp target/release/auth_service \
          target/release/movie_service \
          target/release/song_service \
          target/release/search_service \
          target/release/license_service \
          target/release/notification_service \
          /out/

# Runtime: Debian bookworm-slim; rdkafka is linked statically (cmake-build),
# so no Kafka library ships in the image. One image carries all six
# binaries; deployments select the service via `command:`.
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home service
COPY --from=builder /out/ /usr/local/bin/
USER service
# Services read PORT (deployments set 8101..8106) and expose /healthz.
ENV PORT=8100
EXPOSE 8100
# No ENTRYPOINT: the orchestrator names the binary (command: [auth_service]).
