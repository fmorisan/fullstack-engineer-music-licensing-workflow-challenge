#!/usr/bin/env bash
# Playwright negotiation spec against the running stack (make up + make seed).
set -euo pipefail
cd "$(dirname "$0")/../../../frontend/e2e"

[ -d node_modules ] || npm install
# Idempotent browser install; the suite fails with a clear error if missing.
npx playwright install chromium >/dev/null 2>&1 || true
exec npx playwright test
