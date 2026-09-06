# Music Licensing Workflow — root orchestration.
#
# Make is the standard entrypoint; plain bash scripts (infrastructure/scripts/)
# carry anything nontrivial so every recipe stays a reviewable one-liner.
# Backend tooling is cargo-based; JS tooling is confined to frontend/.

# Container tooling resolves at parse time: podman when installed, docker
# otherwise. Both overridable, e.g.: make up COMPOSE="docker compose" BUILDER=docker
BUILDER := $(shell command -v podman >/dev/null 2>&1 && echo podman || echo docker)
COMPOSE := $(shell command -v podman >/dev/null 2>&1 && echo 'podman compose' || echo 'docker compose')

COMPOSE_FILE := infrastructure/docker/compose.yml
HOST_OVERRIDES := -f infrastructure/docker/compose.yml -f infrastructure/docker/compose.host-services.yml
FULL_STACK := -f infrastructure/docker/compose.yml -f infrastructure/docker/compose.services.yml

# Which stack mode is live (host-dev vs full), resolved per invocation.
MODE := $(shell bash infrastructure/scripts/active_mode.sh)
FILES := $(if $(filter host,$(MODE)),$(HOST_OVERRIDES),$(FULL_STACK))

SERVICES := auth_service movie_service song_service search_service license_service notification_service

.DEFAULT_GOAL := help

.PHONY: help build test test-integration prepare fmt fmt-check lint check $(SERVICES) \
        frontend-install frontend-dev frontend-build frontend-lint \
        images up down nuke up-host ps restart logs seed e2e coverage coverage-html \
        k8s-up k8s-down k8s-ps

help: ## Show this help
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_$][a-zA-Z0-9_$${()}\/ -]*:.*## / \
	  { printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# ─── Rust workspace ──────────────────────────────────────────────────────────

build: ## Build all workspace crates
	cargo build --workspace

test: ## Run all tests (integration tests need a working container environment)
	cargo test --workspace

test-integration: test ## Alias of `test` (kept for discoverability)

prepare: ## Regenerate per-service .sqlx offline metadata against the local stack
	bash infrastructure/scripts/prepare.sh

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting without writing
	cargo fmt --all -- --check

lint: ## Lint with clippy, warnings as errors
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt-check lint test build ## Full local gate (what CI runs)

# Run one service on the host against the up-host stack, e.g. `make auth_service`.
$(SERVICES): ## Run <service> locally (needs `make up-host`)
	@bash infrastructure/scripts/dev.sh $@

# ─── Frontend ────────────────────────────────────────────────────────────────

frontend-install: ## Install frontend dependencies
	cd frontend && npm install

frontend-dev: ## Vite dev server (http://localhost:5173)
	cd frontend && npm run dev

frontend-build: ## Typecheck + production build
	cd frontend && npm run build

frontend-lint: ## ESLint
	cd frontend && npm run lint

# ─── Infrastructure ───────────────────────────────────────────────────────────
#
# Three modes:
#   make up       full stack: infra + six services + frontend (containers)
#   make up-host  infra only; Kong routes to host-run `make <service>`
#   make <service>  run one service on the host against the up-host stack

images: ## Build all images (one shared service image + frontend)
	@echo "==> image acme-services:dev (six binaries, one build)"
	$(BUILDER) build -f infrastructure/docker/services.Dockerfile -t acme-services:dev .
	@echo "==> image acme-frontend:dev"
	$(BUILDER) build -f infrastructure/docker/frontend.Dockerfile -t acme-frontend:dev .

up: images ## Start the FULL stack, healthchecked; bootstrap MinIO buckets. Reviewer mode.
	$(COMPOSE) $(FULL_STACK) up -d --no-build --wait
	$(COMPOSE) $(FULL_STACK) run --rm minio-init

down: ## Stop the stack (volumes preserved)
	$(COMPOSE) -f $(COMPOSE_FILE) down --remove-orphans

nuke: ## Stop the stack and delete volumes (fresh databases/media)
	$(COMPOSE) -f $(COMPOSE_FILE) down -v --remove-orphans

up-host: ## Start infra only; Kong routes to host-run `make <service>`
	$(COMPOSE) $(HOST_OVERRIDES) up -d --wait
	$(COMPOSE) $(HOST_OVERRIDES) run --rm minio-init

ps: ## Stack status (of the active mode)
	$(COMPOSE) $(FILES) ps

restart: ## Restart one stack service (re-reads bind-mounted configs). Usage: make restart SERVICE=kong
	$(COMPOSE) $(FILES) up -d --force-recreate --no-deps $(SERVICE)

logs: ## Tail stack logs. Usage: make logs [SERVICE=kafka]
	$(COMPOSE) $(FILES) logs -f $(SERVICE)

seed: ## Seed demo data through the gateway (idempotent; needs the stack up)
	bash infrastructure/docker/seed/seed.sh

e2e: ## End-to-end negotiation spec against the running stack (make up + make seed)
	@bash infrastructure/scripts/e2e.sh

# ─── Coverage ─────────────────────────────────────────────────────────────────

coverage: ## Coverage gate: full test suite with ratcheted line-coverage floors
	@bash -euo pipefail -c 'cargo llvm-cov --summary-only --ignore-filename-regex "/main\.rs$$" 2>/dev/null | python3 scripts/coverage_gate.py'

coverage-html: ## Full HTML coverage report (target/llvm-cov/html/index.html)
	@bash -euo pipefail -c 'cargo llvm-cov --html --output-path target/llvm-cov/html/index.html --ignore-filename-regex "/main\.rs$$" 2>/dev/null && echo "report: target/llvm-cov/html/index.html"'

# ─── Kubernetes (minikube) ────────────────────────────────────────────────────

k8s-up: ## Deploy the full stack onto minikube (localhost URLs match compose)
	@bash -euo pipefail -c 'curl -s -o /dev/null http://localhost:5173/ && { echo "something is serving :5173 — stop the compose stack first (make down)" >&2; exit 1; }; exec bash infrastructure/k8s/up.sh'

k8s-down: ## Stop port-forwards and delete the k8s namespace (cluster kept)
	bash infrastructure/k8s/down.sh

k8s-ps: ## Kubernetes pod status for the stack
	kubectl -n acme-licensing get pods -o wide
