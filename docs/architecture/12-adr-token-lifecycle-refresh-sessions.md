# ADR-012: Token lifecycle — 15-minute access tokens and rotating refresh sessions

**Status:** Accepted

## Context

Access tokens are stateless RS256 JWTs verified without any server-side
lookup (ADR-011). That makes them impossible to revoke before expiry: the
classic JWT tradeoff. A request to "log out everywhere" cannot invalidate an
already-issued token without reintroducing state onto the per-request
verification path — a revocation denylist checked on every request would put
a network hop and a hot dependency (Redis or auth_service) right back where
we just removed them.

Meanwhile consumers need sessions that survive page reloads and hours of use,
so simply extending the access-token TTL is not an answer either.

## Decision

| | Access token | Refresh token |
|---|---|---|
| Format | RS256 JWT, stateless | Opaque 32 random bytes (base64url) |
| Lifetime | **15 minutes** | 14 days (`REFRESH_TOKEN_TTL_DAYS`) |
| Transport | `Authorization: Bearer`; `?access_token=` for SSE | HttpOnly `SameSite=Lax` cookie, `Path=/auth` |
| Storage | JS memory only (never persisted) | auth_service DB, **SHA-256 hashed** |
| Rotation | n/a | One-time use; every refresh issues a replacement |
| Revocation | None (dies in ≤15 min) | Family-scoped, instant |

1. **Login/register** opens a *refresh family*: the first token row carries a
   `family_id` inherited by every rotation.
2. **`POST /auth/refresh`** (public route) rotates the token in one
   transaction: the old row is marked `rotated_at`/`replaced_by`, the
   replacement (same family) is inserted, and expired rows for the user are
   lazily purged. A new 15-minute access token accompanies the new cookie.
3. **Replay detection:** presenting an already-rotated token is treated as
   theft — the entire family is revoked immediately (the legitimate
   holder's next refresh fails and re-authentication is required).
4. **`POST /auth/logout`** ends the *session*: it revokes the token family
   and clears the cookie. It deliberately does not attempt to revoke the
   access token; residual access after logout is bounded at **≤15 minutes**.
5. **Ban / password-change / role-change** follow the same pattern: revoke
   all of the user's refresh families; claims refresh on the next token
   minted; current access tokens expire within 15 minutes. No denylist, no
   per-request state, anywhere.
6. Active clients refresh ~4×/hour — auth traffic, auth_service's legitimate
   load. The SPA keeps the access token in memory only (XSS cannot read the
   HttpOnly cookie) and silently refreshes on boot and on 401.

## Consequences

**Positive**
- Hard revocation where it matters (sessions) with zero per-request cost.
- Stolen access tokens self-destruct in ≤15 minutes with no renewal path if
  the refresh cookie was not also stolen; a stolen refresh token triggers
  family revocation the moment the thief and owner race.
- Cookie-scoped to `/auth`, HttpOnly, SameSite=Lax: minimal CSRF and XSS
  surface.

**Negative**
- Up to 15 minutes of residual access after logout/ban — accepted and
  documented; instant revocation would require a per-request denylist,
  rejected explicitly (see Context).
- More refresh traffic (~4×/hour per active user) — negligible.
- Refresh cookies need `Secure` in production (`COOKIE_SECURE=true`) behind
  TLS.
