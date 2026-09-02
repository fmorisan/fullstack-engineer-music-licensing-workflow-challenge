# ADR-002: Traefik + forward-auth in place of a dedicated api_gateway service

**Status:** Superseded by [ADR-011](./11-adr-asymmetric-jwts-kong-edge.md)

> The forward-auth design shipped and worked, but the per-request hop to
> auth_service proved redundant for stateless tokens, and OSS Traefik has no
> in-process JWT middleware. ADR-011 replaces Traefik with Kong and moves
> verification to a shared platform middleware + the gateway edge using
> asymmetric keys. The trade-off analysis below is preserved for history.

## Context

The design doc lists `api_gateway` as one of the services. A custom Rust gateway would
mean owning JWT verification, route tables, header injection, and streaming proxy code
(including SSE-safe passthrough). Traefik provides all of this as battle-tested
configuration with a forward-auth middleware pattern.

## Decision

- **Traefik** (container, file provider) is the L7 entry point: TLS termination (local:
  plain HTTP), routing to services, CORS for the frontend dev server, and response
  streaming (SSE works out of the box; the SSE routes are excluded from compression).
- Authentication/authorization is implemented with Traefik's **forward-auth**
  middleware pointing at `auth_service GET /verify?role=<ROLE>`.
  - Valid JWT → `200` + `X-User-Id` / `X-User-Role` response headers, which Traefik
    copies onto the upstream request.
  - Missing/invalid token or insufficient role → `401/403`, request never reaches the
    service.
- Per-route role enforcement is expressed by attaching distinct forward-auth middlewares
  (e.g. `studio-only`, `label-only`) with different `role` query parameters.
- Services on the internal Docker network trust the injected `X-User-*` headers; they are
  not published to the host. This trust boundary is documented and enforced by network
  topology.

## Consequences

**Positive**
- The gateway is configuration, not code: less surface to test and maintain.
- Role policy is visible in one Traefik file instead of spread across a gateway binary.
- auth_service remains the single source of truth for identity.

**Negative**
- Deviation from the design doc's service list (documented here).
- Header-injection trust model requires network discipline (services must never be
  exposed directly). For service-to-service calls we forward the original JWT and
  re-verify, rather than trusting forged headers from arbitrary internal callers.
- Advanced gateway logic (aggregation, per-consumer rate limiting) would eventually
  outgrow Traefik config; acceptable at this scale.
