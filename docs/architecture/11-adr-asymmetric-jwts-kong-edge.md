# ADR-011: Asymmetric JWTs (RS256 + JWKS) with Kong at the edge and per-service verification

**Status:** Accepted

## Context

ADR-002 put authentication at the edge via Traefik forward-auth calling
auth_service `GET /verify`. Two problems emerged:

1. **Redundant hot-path hop.** Tokens are stateless; verifying them needs only
   a public key and no database. A per-request network call to auth_service
   added latency and made auth_service a hot dependency of every request —
   its outage or slowdown would take down the entire platform, and OSS
   Traefik offers no in-process JWT middleware to remove the hop.
2. **Symmetric secret sprawl.** HS256 meant distributing `JWT_SECRET` to every
   verifying party; a leak anywhere allows token forgery everywhere.

Additionally, the Kubernetes/Istio expansion (Phase 11) favors a gateway that
pairs well with service meshes: Kong Gateway (OSS) runs db-less with
declarative config, has a first-class `jwt` plugin (RS256, `exp`/`nbf`
verification, header/cookie/query token sources), and its Kubernetes Ingress
Controller deploys the same policies as CRDs.

## Decision

1. **Tokens are RS256.** auth_service signs with a PKCS#8 private key
   (`JWT_PRIVATE_KEY_FILE`); nothing else ever sees it. Every verifier —
   Kong and each service — holds only the public key.
2. **auth_service publishes `GET /.well-known/jwks.json`** (kid/kty/n/e/use/
   alg). OSS Kong embeds the public key statically as a Consumer credential
   in `kong.yml` (no JWKS fetch in OSS); production edge products that do
   fetch JWKS (Traefik Hub `jwksUrl`, Kong Enterprise OIDC, Envoy, AWS API
   Gateway) can validate against the same endpoint without redeploying us.
   Key ids derive deterministically from the key material
   (`platform::jwt::key_id`), so all parties agree without coordination.
3. **Dual validation (edge + service).** Kong's `jwt` plugin rejects
   unauthenticated traffic before it reaches the internal network. Services
   *also* verify locally via `platform::auth::require_auth` with the same
   public key (~µs, no network, no shared state): the platform keeps
   zero-trust semantics internally, the gateway is hardening rather than a
   security dependency, and local development (OSS Kong, host-run services)
   behaves exactly like production.
4. **Public routes** (login, register, refresh, logout, jwks) carry no jwt
   plugin; CORS terminates preflights before auth evaluation. EventSource
   clients may pass `?access_token=` (the platform middleware and Kong both
   accept it).
5. **Dev keys are committed fixtures** (`infrastructure/docker/keys/`) so
   compose, `make <service>`, and CI are deterministic. Production generates its
   own keypair, stores the private key in a secret manager, and rotates by
   introducing a second key and retiring the old one after token TTLs lapse.

This supersedes **ADR-002** (forward-auth); Traefik is removed in favor of
Kong.

## Consequences

**Positive**
- No per-request hop to auth_service; it handles auth traffic only
  (login/refresh) — its actual job.
- Secret distribution problem eliminated: verifiers hold public keys only.
- Portable issuer contract (JWKS) for any production edge; Kong KIC/Istio
  path preserved for Kubernetes.
- Identity logic lives once, in tested Rust (`platform`), instead of gateway
  configuration.

**Negative**
- Dual validation means each service carries the public key and middleware
  (centralized once in `platform`; trivial per service).
- Dev private key is committed (labeled fixture; production section
  documents the real flow).
- OSS Kong cannot fetch JWKS, so key rotation requires re-rendering
  `kong.yml` (scripted) until a JWKS-capable edge is adopted.
