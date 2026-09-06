#!/usr/bin/env bash
# Regenerate per-service .sqlx offline query metadata against the local
# stack (localhost:5433). Each service's database must exist and be
# migrated — services migrate at boot, or run the stack once.
set -euo pipefail
cd "$(dirname "$0")/../.."

export SQLX_OFFLINE=false
for dir in services/*; do
  if [ -d "$dir/migrations" ]; then
    svc=$(basename "$dir" | sed 's/_service//')
    echo "==> preparing $dir against ${svc}_db"
    (cd "$dir" && DATABASE_URL="postgres://acme:acme_dev_only@localhost:5433/${svc}_db" cargo sqlx prepare)
  fi
done
