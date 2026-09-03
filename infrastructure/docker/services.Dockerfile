# Shared builder for all Rust services (parameterized by SERVICE).
#
#   podman build -f infrastructure/docker/services.Dockerfile \
#     --build-arg SERVICE=auth_service -t acme-auth:dev .
#
# cargo-chef caches dependency compilation across services: the planner and
# cooker layers are identical for every service, so building the second
# service onwards only links its binary.
#
# Runtime: Debian bookworm-slim. rdkafka's bundled librdkafka is linked
# STATICALLY on Linux (cmake-build; see the root Cargo.toml note), so the
# runtime image needs no Kafka library.

FROM rust:1.96-bookworm AS chef
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        cmake g++ libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Cap parallelism: unbounded rustc jobs OOM smaller build VMs.
ENV CARGO_BUILD_JOBS=3
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies only (cached layer).
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ARG SERVICE
RUN cargo build --release --bin "${SERVICE}"

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home service
ARG SERVICE
COPY --from=builder "/app/target/release/${SERVICE}" /usr/local/bin/service
USER service
# Services read PORT (defaults 8101..8106) and expose /healthz.
ENV PORT=8100
EXPOSE 8100
ENTRYPOINT ["/usr/local/bin/service"]
