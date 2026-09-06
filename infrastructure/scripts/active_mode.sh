#!/usr/bin/env bash
# Print the ACTIVE stack mode's marker: "host" when Kong runs the host-dev
# config (up-host mode), "full" otherwise (including when nothing runs).
# The Makefile maps this onto its compose file lists.
set -euo pipefail

bin=$(command -v podman || command -v docker || true)
if [ -n "$bin" ]; then
  if "$bin" inspect acme-licensing-kong-1 2>/dev/null \
     | grep -q 'KONG_DECLARATIVE_CONFIG=/kong/kong.host-dev.yml'; then
    echo host
    exit 0
  fi
fi
echo full
