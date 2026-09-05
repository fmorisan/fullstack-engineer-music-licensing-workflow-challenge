# Music Licensing Workflow — root orchestration
# All backend tooling is cargo-based; JS tooling is confined to frontend/.

set dotenv-load

# Compose runner: defaults to podman locally; CI exports COMPOSE="docker compose"
COMPOSE := env_var_or_default("COMPOSE", "podman compose")
# Image builder: sequential builds share the cargo-chef cache layer and
# avoid OOM'ing smaller VMs (parallel compose builds duplicate the cook).
BUILDER := env_var_or_default("BUILDER", "podman")
SERVICES := "auth_service movie_service song_service search_service license_service notification_service"
COMPOSE_FILE := "infrastructure/docker/compose.yml"
HOST_OVERRIDES := "-f infrastructure/docker/compose.yml -f infrastructure/docker/compose.host-services.yml"
FULL_STACK := "-f infrastructure/docker/compose.yml -f infrastructure/docker/compose.services.yml"

default:
    @just --list

# ─── Rust workspace ──────────────────────────────────────────────────────────

# Build all workspace crates
build:
    cargo build --workspace

# Resolve a HEALTHY podman machine socket, restarting the machine when the
# API socket has been reaped by macOS temp-dir cleanup. Prints nothing when
# docker is available or podman is unusable.
# [private]
_healthy_podman_socket:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v docker >/dev/null 2>&1 && exit 0
    command -v podman >/dev/null 2>&1 || exit 0
    sock=$(podman machine inspect 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["ConnectionInfo"]["PodmanSocket"]["Path"])' || true)
    healthy() { [ -n "$sock" ] && [ -S "$sock" ] && curl -s --unix-socket "$sock" http://localhost/_ping >/dev/null 2>&1; }
    if ! healthy; then
        echo "podman API socket unhealthy; restarting the machine..." >&2
        podman machine stop >/dev/null 2>&1 || true
        podman machine start >/dev/null 2>&1 || true
        sock=$(podman machine inspect 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["ConnectionInfo"]["PodmanSocket"]["Path"])' || true)
        healthy || sock=""
    fi
    [ -n "$sock" ] && echo "unix://$sock" || true

# Run all tests, including testcontainer-based integration tests.
# Resolves (and if needed heals) the podman machine socket automatically.
test:
    #!/usr/bin/env bash
    set -euo pipefail
    sock="$(just --quiet _healthy_podman_socket)"
    if [ -n "$sock" ]; then
        export DOCKER_HOST="$sock"
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
    db=$(echo "{{ service }}" | sed 's/_service//')_db
    export DATABASE_URL="${DATABASE_URL:-postgres://acme:acme_dev_only@localhost:5433/$db}"
    export JWT_PRIVATE_KEY_FILE="${JWT_PRIVATE_KEY_FILE:-infrastructure/docker/keys/dev-auth-private.pem}"
    export JWT_PUBLIC_KEY_FILE="${JWT_PUBLIC_KEY_FILE:-infrastructure/docker/keys/dev-auth-public.pem}"
    export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"
    cargo run -p {{ service }}

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
#
# Three modes:
#   just up       full stack: infra + six services + frontend (containers)
#   just up-host  infra only; Kong routes to host-run `just dev <service>`
#   just dev X    run one service on the host against the up-host stack

# Detect which stack mode is live by inspecting Kong's declarative config
# (host-dev mode swaps in kong.host-dev.yml). Prints the compose file set
# for the ACTIVE mode; full stack when ambiguous/not running.
# [private]
_active_compose_files:
    #!/usr/bin/env bash
    set -euo pipefail
    if docker="$(command -v docker)" 2>/dev/null || docker="$(command -v podman)"; then
        mode=$("$docker" inspect acme-licensing-kong-1 2>/dev/null | python3 -c 'import json,sys; envs=json.load(sys.stdin)[0]["Config"]["Env"]; print("\n".join(e for e in envs if e.startswith("KONG_DECLARATIVE_CONFIG=")))' || true)
        if echo "$mode" | grep -q 'host-dev'; then
            echo "{{ HOST_OVERRIDES }}"
            exit 0
        fi
    fi
    echo "{{ FULL_STACK }}"

# Build all service + frontend images sequentially (chef layers shared).
images:
    #!/usr/bin/env bash
    set -euo pipefail
    for svc in {{ SERVICES }}; do
        echo "==> image acme-$svc:dev"
        {{ BUILDER }} build -f infrastructure/docker/services.Dockerfile \
            --build-arg SERVICE="$svc" -t "acme-$svc:dev" .
    done
    echo "==> image acme-frontend:dev"
    {{ BUILDER }} build -f infrastructure/docker/frontend.Dockerfile -t acme-frontend:dev .

# Start the FULL stack (infra + services + frontend), building images if
# needed, wait for healthchecks, bootstrap MinIO buckets. Reviewer mode.
up: images
    {{ COMPOSE }} {{ FULL_STACK }} up -d --no-build --wait
    {{ COMPOSE }} {{ FULL_STACK }} run --rm minio-init

# Stop the stack (volumes preserved)
down:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ COMPOSE }} -f {{ COMPOSE_FILE }} down --remove-orphans

# Stop the stack and delete volumes (fresh databases/media)
nuke:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ COMPOSE }} -f {{ COMPOSE_FILE }} down -v --remove-orphans

# Start the stack in host-dev mode: Kong routes to services running on the
# host via `just dev <service>` (host.containers.internal)
up-host:
    {{ COMPOSE }} {{ HOST_OVERRIDES }} up -d --wait
    {{ COMPOSE }} {{ HOST_OVERRIDES }} run --rm minio-init

# Stack status (of the active mode)
ps:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ COMPOSE }} $(just --quiet _active_compose_files) ps

# Restart one stack service; re-reads bind-mounted configs (inotify does not
# cross the podman VM boundary, so config edits need a force-recreate) and
# restarts in whichever mode is currently active.
restart service:
    #!/usr/bin/env bash
    set -euo pipefail
    {{ COMPOSE }} $(just --quiet _active_compose_files) up -d --force-recreate --no-deps {{ service }}

# Tail stack logs (optionally one service: `just logs kafka`)
logs service='':
    #!/usr/bin/env bash
    set -euo pipefail
    {{ COMPOSE }} $(just --quiet _active_compose_files) logs -f {{ service }}

# Seed demo data through the gateway (idempotent; needs the stack up)
seed:
    bash infrastructure/docker/seed/seed.sh

# End-to-end negotiation spec against the running stack (just up + just seed).
e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    cd frontend/e2e
    [ -d node_modules ] || npm install
    npx playwright install chromium >/dev/null 2>&1 || true
    npx playwright test

# Coverage gate: full test suite with line-coverage floors (ratcheted).
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    sock="$(just --quiet _healthy_podman_socket)"
    if [ -n "$sock" ]; then
        export DOCKER_HOST="$sock"
    fi
    cargo llvm-cov --summary-only --ignore-filename-regex '/main\.rs$' 2>/dev/null \
        | python3 scripts/coverage_gate.py

# Full HTML coverage report (non-blocking artifact).
coverage-html:
    #!/usr/bin/env bash
    set -euo pipefail
    sock="$(just --quiet _healthy_podman_socket)"
    if [ -n "$sock" ]; then
        export DOCKER_HOST="$sock"
    fi
    cargo llvm-cov --html --output-path target/llvm-cov/html/index.html \
        --ignore-filename-regex '/main\.rs$' 2>/dev/null
    echo "report: target/llvm-cov/html/index.html"

# Deploy the full stack onto minikube (localhost URLs match compose).
k8s-up:
    #!/usr/bin/env bash
    set -euo pipefail
    if curl -s -o /dev/null http://localhost:5173/; then
        echo "something is serving :5173 — stop the compose stack first (just down)" >&2
        exit 1
    fi
    bash infrastructure/k8s/up.sh

# Stop port-forwards and delete the k8s namespace (cluster kept).
k8s-down:
    bash infrastructure/k8s/down.sh

# Kubernetes pod status for the stack.
k8s-ps:
    kubectl -n acme-licensing get pods -o wide
