# Music Licensing Workflow — root orchestration
# All backend tooling is cargo-based; JS tooling is confined to frontend/.

set dotenv-load

# Compose runner: defaults to podman locally; CI exports COMPOSE="docker compose"
COMPOSE := env_var_or_default("COMPOSE", "podman compose")
COMPOSE_FILE := "infrastructure/docker/compose.yml"

default:
    @just --list

# ─── Rust workspace ──────────────────────────────────────────────────────────

# Build all workspace crates
build:
    cargo build --workspace

# Run all unit tests
test:
    cargo test --workspace

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
