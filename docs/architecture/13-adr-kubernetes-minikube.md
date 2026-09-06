# ADR-013: Kubernetes deployment (minikube) with port-forward parity

**Status:** Accepted

## Context

The reviewer experience runs on compose (`make up`), but the challenge's
bonus track asks for Kubernetes literacy, and minikube is available locally.
The stack has a dozen components (six services, gateway, four stateful
backing stores, email capture) and one hard constraint inherited from the
compose design: **URLs baked into the frontend at build time** —
`VITE_API_URL=http://localhost:8080` and media at `http://localhost:9000` —
plus a Kong CORS allowlist for `http://localhost:5173` and pre-signed S3
URLs whose host is chosen by `S3_ENDPOINT`.

## Decision

One kustomize base (`infrastructure/k8s/base`) deploys the entire stack into
namespace `acme-licensing`, wrapped by `make k8s-up` / `make k8s-down`.
No Helm: the manifests stay directly readable, and there is a single target
environment.

**Port-forward parity.** The four user-facing services are reached through
`kubectl port-forward` on the exact compose ports — frontend :5173, gateway
:8080, MinIO :9000, Mailpit :8025. The compose-built images therefore work
unchanged: no rebuild, no CORS changes, no `VITE_*` templating. `S3_ENDPOINT`
stays `http://localhost:9000` — pre-signing is offline signature math, so the
endpoint only names the host baked into signed URLs, which the browser
reaches through the forward (same reasoning as the compose fix for
browser-unreachable presigned hosts).

**Side-loaded images.** Images are built by the existing compose flow
(`make images`) and travel into the cluster explicitly:
`podman save → minikube cp → ctr -n k8s.io images import`, because
`minikube image load` does not see this podman store. Consequences:

- Manifests use fully qualified `localhost/acme-<service>:dev` names with
  `imagePullPolicy: Never` — nothing ever attempts a registry pull.
- Manifests are applied **before** the import: kubelet's image GC can evict
  unreferenced images while no pod wants them; pending `ErrImageNeverPull`
  pods pin images the moment they land.
- `minikube ssh` returns nonzero even on successful imports; the script
  verifies presence instead of trusting exit codes.

**Kafka is a headless service with `publishNotReadyAddresses: true`.** The
KRaft quorum voter is `1@kafka:9093`, dialed during broker boot — before
readiness would admit the pod to a ClusterIP service's endpoints. With a
virtual IP, the broker can never elect itself (self-deadlock); headless DNS
resolves straight to the pod IP regardless of readiness, and clients resolve
`kafka:9092` the same way. Kafka's readiness probe is a plain TCP socket
check: the broker-API script spins up a JVM per probe and blows the default
one-second probe timeout.

**Everything else mirrors compose.** Service names use dashes
(`auth-service`, DNS-1035) and Kong's declarative config is rendered into a
k8s variant with those hostnames (`kong.k8s.yml`, from the same template and
render script). The dev JWT keypair rides in the `acme-keys` secret. Boot
ordering is left to Kubernetes: services crash-retry until their databases
and brokers answer, migrations run at boot, and the consumer zombie-guards
(see the notification/search rebuild fixes) handle broker churn.

## Consequences

- The Playwright e2e suite passes against the k8s deployment unchanged —
  same localhost URLs — which is the deployment's acceptance test.
- Port-forwards bind the same localhost ports as compose: the two modes are
  mutually exclusive (`make down` first), which `k8s-up` checks.
- `k8s-down` deletes the namespace but keeps the cluster: images stay
  loaded, so a redeploy is `kubectl apply` plus seconds, not minutes.
- Single replicas, single-node brokers, dev credentials and a committed
  dev keypair: this is the compose posture on a different scheduler, not a
  production topology. PVCs (default StorageClass) hold Postgres, Kafka,
  ES, and MinIO data for the cluster's lifetime.
