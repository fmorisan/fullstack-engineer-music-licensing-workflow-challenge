# ADR-008: Vite + React SPA frontend

**Status:** Accepted

## Context

The design doc specifies "React application, compiled with Vite to be deployed in a
static manner on CloudFront". The backend is Rust (ADR-001); JS/TS is confined to the
frontend by mandate.

## Decision

- **Vite + React + TypeScript** SPA, built to static assets:
  - React Router for client-side routing.
  - **TanStack Query** for server state (caching, refetch on focus/reconnect — pairs
    with SSE gaps, see ADR-005).
  - `EventSource` hook per real-time feed (licenses + notifications).
  - No Redux; local state via React hooks, server state via Query.
- Dev mode: `vite dev` proxies `/api` to Traefik. Static build served by nginx in the
  compose stack (production-parity).
- The frontend's `package.json` is the **only** JS/TS toolchain in the repository; there
  is no root package.json, no Turborepo/pnpm workspace at root. Cross-stack orchestration
  is done by `Makefile` targets.

## Consequences

**Positive**
- Matches the design doc's deployment story exactly (static CDN).
- Fast dev loop; minimal moving parts; no SSR complexity to justify.

**Negative**
- No SSR/SEO — irrelevant for an authenticated internal tool.
- SPA needs the API up for meaningful use; accepted.
