#!/usr/bin/env bash
# Run one service on the host against the up-host stack: `make <service>`.
# Env vars default to the compose dev posture; export your own to override.
set -euo pipefail

service="${1:?usage: dev.sh <service> (e.g. auth_service)}"
case "$service" in
  auth_service|movie_service|song_service|search_service|license_service|notification_service) ;;
  *) echo "unknown service: $service" >&2; exit 1 ;;
esac

db=$(echo "$service" | sed 's/_service//')_db
export DATABASE_URL="${DATABASE_URL:-postgres://acme:acme_dev_only@localhost:5433/$db}"
export JWT_PRIVATE_KEY_FILE="${JWT_PRIVATE_KEY_FILE:-infrastructure/docker/keys/dev-auth-private.pem}"
export JWT_PUBLIC_KEY_FILE="${JWT_PUBLIC_KEY_FILE:-infrastructure/docker/keys/dev-auth-public.pem}"
export SQLX_OFFLINE="${SQLX_OFFLINE:-true}"

exec cargo run -p "$service"
