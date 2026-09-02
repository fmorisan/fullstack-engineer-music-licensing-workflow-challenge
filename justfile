# Music Licensing Workflow — root orchestration
# All backend tooling is cargo-based; JS tooling is confined to frontend/.

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

# ─── Infrastructure (Phase 1) ─────────────────────────────────────────────────
# Targets below are wired up when infrastructure/docker/compose.yml lands.

# Start the local stack
up:
    @echo "not yet available: lands in Phase 1 (infrastructure/docker/compose.yml)"

# Stop the local stack
down:
    @echo "not yet available: lands in Phase 1 (infrastructure/docker/compose.yml)"
