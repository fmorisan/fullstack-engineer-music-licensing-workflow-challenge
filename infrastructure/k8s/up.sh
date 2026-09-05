#!/usr/bin/env bash
# Deploy the full stack onto minikube (ADR-013).
#
# Loads the pre-built acme-* images, renders Kong's k8s config, applies
# the kustomize base, waits for health, and port-forwards the four
# user-facing services — so localhost URLs match the compose stack
# exactly (frontend :5173, gateway :8080, MinIO :9000, Mailpit :8025).
#
# Requires: the compose stack stopped (same localhost ports), images
# built (`just images`).
set -euo pipefail
cd "$(dirname "$0")/../.."

NS=acme-licensing
# Podman if installed, docker otherwise; explicit error beats a silent
# fallback to a tool that doesn't exist on this machine.
if [ -z "${PODMAN:-}" ]; then
  PODMAN=$(command -v podman || command -v docker || true)
fi
[ -n "$PODMAN" ] || { echo "neither podman nor docker found in PATH" >&2; exit 1; }
FORWARD_DIR=/tmp/acme-k8s

echo "==> minikube"
if ! minikube status >/dev/null 2>&1; then
  minikube start --driver="${MINIKUBE_DRIVER:-podman}" --memory 8192 --cpus 4
fi

echo "==> rendering kong config + keys"
python3 infrastructure/docker/render_kong_config.py >/dev/null

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f - >/dev/null
kubectl -n "$NS" create secret generic acme-keys \
  --from-file=private=infrastructure/docker/keys/dev-auth-private.pem \
  --from-file=public=infrastructure/docker/keys/dev-auth-public.pem \
  --dry-run=client -o yaml | kubectl apply -f - >/dev/null
kubectl -n "$NS" create configmap kong-config \
  --from-file=kong.yml=infrastructure/docker/kong/kong.k8s.yml \
  --dry-run=client -o yaml | kubectl apply -f - >/dev/null

# Apply BEFORE loading images: kubelet's containerd garbage-collects
# unreferenced side-loaded images, so pods (imagePullPolicy: Never, waiting
# in ErrImageNeverPull) must exist first — they start as images land.
echo "==> applying manifests"
kubectl apply -k infrastructure/k8s/base

echo "==> loading images"
# minikube's image loader misses the podman store on this setup, so the
# images travel explicitly: podman save → ctr import in the VM. Names are
# fully qualified (localhost/…) and the manifests pin imagePullPolicy:
# Never, so nothing ever tries a registry. One services image carries all
# six binaries (deployments select via command:).
for image in services frontend; do
  echo "    acme-${image}:dev"
  tar=/tmp/acme-${image}.tar
  "$PODMAN" save -q -o "$tar" "localhost/acme-${image}:dev"
  minikube cp "$tar" "/tmp/acme-${image}.tar"
  # minikube ssh returns nonzero even on successful imports.
  minikube ssh "sudo ctr -n k8s.io images import /tmp/acme-${image}.tar" >/dev/null 2>&1 || true
  rm -f "$tar"
  minikube ssh "rm -f /tmp/acme-${image}.tar" >/dev/null 2>&1 || true
done

echo "==> waiting for rollout (services retry their deps; first boot runs migrations)"
kubectl -n "$NS" wait --for=condition=available deploy --all --timeout=600s
kubectl -n "$NS" wait --for=condition=complete job/minio-init --timeout=300s

echo "==> port-forwards (localhost URLs identical to compose)"
mkdir -p "$FORWARD_DIR"
for spec in "kong 8080:8000" "frontend 5173:80" "minio 9000:9000" "mailpit 8025:8025" "grafana 3000:3000"; do
  set -- $spec
  svc=$1; mapping=$2
  if [ -f "$FORWARD_DIR/$svc.pid" ] && kill -0 "$(cat "$FORWARD_DIR/$svc.pid")" 2>/dev/null; then
    echo "    $svc already forwarded"
    continue
  fi
  kubectl -n "$NS" port-forward "svc/$svc" "$mapping" >"$FORWARD_DIR/$svc.log" 2>&1 &
  echo $! >"$FORWARD_DIR/$svc.pid"
  echo "    $svc → localhost:${mapping%%:*}"
done

sleep 3
echo
echo "frontend   http://localhost:5173"
echo "gateway    http://localhost:8080"
echo "mailpit    http://localhost:8025"
echo "minio      http://localhost:9001 (console; forward it if needed)"
echo "grafana    http://localhost:3000 (ACME Licensing — System dashboard)"
echo "prometheus svc/prometheus:9090 (forward it if needed)"
echo
echo "next: just seed   # demo data through the gateway"
