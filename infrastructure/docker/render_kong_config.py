#!/usr/bin/env python3
"""Render kong.yml with the dev public key embedded.

The Kong declarative config must carry the issuer's public key statically
(OSS Kong has no JWKS fetch). This script exists so the key never has to be
hand-copied: it reads keys/dev-auth-public.pem and writes kong/kong.yml from
kong.yml.template. Run from infrastructure/docker:

    python3 render_kong_config.py
"""

from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = HERE / "kong" / "kong.yml.template"
PUBLIC_KEY = HERE / "keys" / "dev-auth-public.pem"
TARGET = HERE / "kong" / "kong.yml"
TARGET_HOST_DEV = HERE / "kong" / "kong.host-dev.yml"


def render(rewrite_service_hosts: bool) -> str:
    public_key = PUBLIC_KEY.read_text().strip()
    indented = "\n".join(f"          {line}" for line in public_key.splitlines())
    body = TEMPLATE.read_text().replace("{{RSA_PUBLIC_KEY}}", indented)
    if rewrite_service_hosts:
        # Point services at the host so Kong can reach `just dev` processes
        # running outside the container network.
        for service, port in [
            ("auth_service", 8101),
            ("movie_service", 8102),
            ("song_service", 8103),
            ("search_service", 8104),
            ("license_service", 8105),
            ("notification_service", 8106),
        ]:
            body = body.replace(
                f"http://{service}:{port}",
                f"http://host.containers.internal:{port}",
            )
    return body


def main() -> None:
    TARGET.write_text(render(rewrite_service_hosts=False))
    print(f"wrote {TARGET} ({TARGET.stat().st_size} bytes)")
    TARGET_HOST_DEV.write_text(render(rewrite_service_hosts=True))
    print(f"wrote {TARGET_HOST_DEV} ({TARGET_HOST_DEV.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
