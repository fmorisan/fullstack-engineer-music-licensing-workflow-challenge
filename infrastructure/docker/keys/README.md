# Development-only signing keys

These fixtures exist so the local compose stack, `just dev`, and CI are fully
deterministic with zero bootstrap steps. **They are not secrets.**

- `dev-auth-private.pem` — PKCS#8 private key; `auth_service` signs with it
  (`JWT_PRIVATE_KEY_FILE`).
- `dev-auth-public.pem` — matching public key; embedded in
  `../kong/kong.yml` and used by services (`JWT_PUBLIC_KEY_FILE`) to verify
  tokens.

Any deployment beyond local development must generate its own keypair, keep
the private key in a secret manager (KMS/Vault), and distribute the public
key via Kong credentials (Admin API/deck/Konnect) or the JWKS endpoint
(`/.well-known/jwks.json`). Rotate by introducing a second key, minting with
the new one, and retiring the old after token TTLs lapse (see
`docs/architecture/11-adr-asymmetric-jwts-kong-edge.md`).

Regenerate locally with:

```sh
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out dev-auth-private.pem
openssl pkey -in dev-auth-private.pem -pubout -out dev-auth-public.pem
```
