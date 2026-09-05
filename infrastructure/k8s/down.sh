#!/usr/bin/env bash
# Tear down the minikube deployment: stop port-forwards, delete the
# namespace (keeps the cluster and its loaded images for fast redeploys).
set -euo pipefail

FORWARD_DIR=/tmp/acme-k8s
if [ -d "$FORWARD_DIR" ]; then
  for pidfile in "$FORWARD_DIR"/*.pid; do
    [ -f "$pidfile" ] || continue
  kill "$(cat "$pidfile")" 2>/dev/null || true
  done
  rm -rf "$FORWARD_DIR"
  echo "port-forwards stopped"
fi

kubectl delete namespace acme-licensing --ignore-not-found --wait=true
echo "namespace deleted (cluster kept; minikube delete removes everything)"
