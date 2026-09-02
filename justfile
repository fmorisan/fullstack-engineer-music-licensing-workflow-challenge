# Music Licensing Workflow — root orchestration
# All backend tooling is cargo-based; JS tooling is confined to frontend/.

set dotenv-load

# Compose runner: defaults to podman locally; CI exports COMPOSE="docker compose"
COMPOSE := env_var_or_default("COMPOSE", "podman compose")
COMPOSE_FILE := "infrastructure/docker/compose.yml"
HOST_OVERRIDES := "-f infrastructure/docker/compose.yml -f infrastructure/docker/compose.host-services.yml"

default:
    @just --list

# ─── Rust workspace ──────────────────────────────────────────────────────────

# Build all workspace crates
build:
    cargo build --workspace

# Run all tests, including testcontainer-based integration tests.
# Resolves the podman machine socket automatically when docker is absent.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v docker >/dev/null 2>&1 && command -v podman >/dev/null 2>&1; then
        sock=$(podman machine inspect 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["ConnectionInfo"]["PodmanSocket"]["Path"])')
        # The inspect-reported -api.sock can be reaped by macOS temp cleanup;
        # fall back to the stable rootless socket in the same directory.
        if [ -n "$sock" ] && [ ! -S "$sock" ]; then
            fallback="$(dirname "$sock")/podman-machine-default.sock"
            [ -S "$fallback" ] && sock="$fallback" || sock=""
        fi
        if [ -n "$sock" ]; then
            export DOCKER_HOST="unix://$sock"
        fi
    fi
    cargo test --workspace

# Alias of `test` (kept for discoverability of the integration suite)
test-integration: test

# Regenerate per-service .sqlx offline query metadata against the local stack.
# Requires each service's database to be migrated (services run migrations at
# boot, or: sqlx migrate run --source services/<svc>_service/migrations).
prepare:
    #!/usr/bin/env bash
    set -euo pipefail
    export SQLX_OFFLINE=false
    for dir in services/*; do
      if [ -d "$dir/migrations" ]; then
        svc=$(basename "$dir" | sed 's/_service//')
        echo "==> preparing $dir against ${svc}_db"
        (cd "$dir" && DATABASE_URL="postgres://acme:acme_dev_only@localhost:5433/${svc}_db" cargo sqlx prepare)
      fi
    done

# Format all code
fmt:
    cargo fmt --all

# Check formatting without writing
fmt-check:
    cargo fmt --all -- --check

# Lint with clippy, warnings as errors
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full local gate (what CI runs)
check: fmt-check lint test build

# Run a single service locally, e.g. `just dev auth_service`
dev service:
    #!/usr/bin/env bash
    set -euo pipefail
    db=$(echo "{{service}}" | sed 's/_service//')_db
    export DATABASE_URL="${DATABASE_URL:-postgres://acme:acme_dev_only@localhost:5433/$db}"
    export JWT_PRIVATE_KEY_FILE="${JWT_PRIVATE_KEY_FILE:-infrastructure/docker/keys/dev-auth-private.pem}"
    export JWT_PUBLIC_KEY_FILE="${JWT_PUBLIC_KEY_FILE:-infrastructure/docker/keys/dev-auth-public.pem}"
    export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
    cargo run -p {{service}}

# ─── Frontend ────────────────────────────────────────────────────────────────

# Install frontend dependencies
frontend-install:
    cd frontend && npm install

# Vite dev server (http://localhost:5173)
frontend-dev:
    cd frontend && npm run dev

# Typecheck + production build
frontend-build:
    cd frontend && npm run build

# ESLint
frontend-lint:
    cd frontend && npm run lint

# ─── Infrastructure ───────────────────────────────────────────────────────────

# Start the local stack, wait for healthchecks, bootstrap MinIO buckets
up:
    {{COMPOSE}} -f {{COMPOSE_FILE}} up -d --wait
    {{COMPOSE}} -f {{COMPOSE_FILE}} run --rm minio-init

# Stop the local stack (volumes preserved)
down:
    {{COMPOSE}} -f {{COMPOSE_FILE}} down

# Stop the local stack and delete volumes (fresh databases/media)
nuke:
    {{COMPOSE}} -f {{COMPOSE_FILE}} down -v

# Start the stack in host-dev mode: Kong routes to services running on the
# host via `just dev <service>` (host.containers.internal)
up-host:
    {{COMPOSE}} {{HOST_OVERRIDES}} up -d --wait
    {{COMPOSE}} {{HOST_OVERRIDES}} run --rm minio-init

# Stack status
ps:
    {{COMPOSE}} -f {{COMPOSE_FILE}} ps

# Restart one stack service; re-reads bind-mounted configs (inotify does not
# cross the podman VM boundary, so config edits need a force-recreate)
restart service:
    {{COMPOSE}} -f {{COMPOSE_FILE}} up -d --force-recreate {{service}}

# Tail stack logs (optionally one service: `just logs kafka`)
logs service='':
    {{COMPOSE}} -f {{COMPOSE_FILE}} logs -f {{service}}
