# ADR-009: UUIDv7 identifiers and MinIO pre-signed media uploads

**Status:** Accepted

## Context

The design doc mandates UUIDv7 for movie/song ids "to enable horizontal scaling", S3 +
CloudFront + pre-signed URLs for media, and notes integer scene ids within a movie are
acceptable.

## Decision

**Identifiers**
- Movies, songs, licenses, notifications, users: **UUIDv7** generated in application
  code (`uuid` crate, `v7` feature) — time-ordered (B-tree friendly) and generatable in
  any replica without coordination.
- Scenes: **integer** `scene_number` unique within a movie (design doc sanctioned);
  scenes are always addressed as `/movies/:movie_id/scenes/:scene_number`.

**Media**
- Local **MinIO** (S3-compatible) replaces S3+CloudFront in development; production
  swaps the endpoint. Buckets: `movie-media` (posters, scene captures),
  `song-media` (box art, ≤30s audio previews).
- Services mint **pre-signed PUT URLs** via `aws-sdk-s3` (endpoint override, forced
  path-style for MinIO) with short expiry and content-type constraints; clients upload
  directly. Objects are addressed by key; public reads go through presigned GET or a
  CDN in production.

## Consequences

**Positive**
- UUIDv7 keeps indexes write-friendly and ids replica-safe.
- Direct-to-storage uploads keep large binaries out of the services entirely.

**Negative**
- App-generated ids require columns defaulting to client-provided values (or trigger
  fallback); discipline needed so no service relies on DB-generated ids for these
  entities.
- MinIO path-style config differs from real S3 virtual-host style — abstracted behind
  one `platform` S3 client builder.
