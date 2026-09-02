# ADR-007: REST-only API surface (no GraphQL)

**Status:** Accepted

## Context

The challenge allows REST and/or GraphQL and asks for justification when only one is
used. The design doc already leans REST-only.

## Decision

The API surface is **REST only**. Rationale:

1. **Shape of the domain**: resources are simple and shallow (movies, scenes, songs,
   licenses, notifications). There are no client-defined aggregation needs; the read
   patterns are known and few (scene tracks with license status is a single join we can
   shape server-side).
2. **Operational cost**: GraphQL adds a schema/type bridge (e.g. async-graphql), query
   complexity management, N+1 dataloaders, and a second auth story for subscriptions —
   real cost for zero current benefit.
3. **Real-time is SSE**, not GraphQL subscriptions (ADR-005), so GraphQL would add
   nothing there.
4. **Ecosystem fit**: Traefik routing, forward-auth headers, and presigned S3 flows are
   all naturally REST-shaped.

## Consequences

**Positive**
- One API style to document, test, and secure; trivial curl-able debugging.
- Less code: no schema stitching or resolver layer.

**Negative**
- Occasional over/under-fetching (e.g. track lists embed license state by design).
- If a heterogeneous-client future emerges (mobile with bespoke views), GraphQL could be
  introduced at the edge without changing services.
