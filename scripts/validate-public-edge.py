#!/usr/bin/env python3

from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


def require(path: Path, fragments: tuple[str, ...]) -> str:
    text = path.read_text(encoding="utf-8")
    for fragment in fragments:
        if fragment not in text:
            raise SystemExit(f"{path.relative_to(ROOT)} is missing {fragment!r}")
    return text


def main() -> int:
    compose = require(
        ROOT / "deploy/docker/edge.compose.yaml",
        (
            'user: "65532:65532"',
            "read_only: true",
            'cap_drop: ["ALL"]',
            "no-new-privileges:true",
            "target: 8080",
            "target: 8443",
            "create_host_path: false",
            "1panel-network:",
        ),
    )
    if "privileged:" in compose or "network_mode: host" in compose:
        raise SystemExit("edge compose must not use privileged or host networking")

    edge = require(
        ROOT / "deploy/docker/xs-nexus-edge.sh",
        (
            "@sha256:[0-9a-f]{64}",
            "edge image must be pinned by SHA-256 digest",
            "validate_private_path",
            "require_application",
            "--pull never",
            "caddy reload",
        ),
    )
    if "docker compose down" in edge or "docker system prune" in edge:
        raise SystemExit("edge lifecycle must not tear down unrelated resources")

    stack = require(
        ROOT / "deploy/docker/xs-nexus-stack.sh",
        (
            "XS_BIND_ADDRESS",
            "XS_UDP_BIND_ADDRESS",
            "HTTP services must bind to 127.0.0.1",
            "UDP services must bind to 127.0.0.1 or 0.0.0.0",
        ),
    )
    if stack.count("HTTP services must bind to 127.0.0.1") != 1:
        raise SystemExit("HTTP loopback gate must be unique")

    caddy = require(
        ROOT / "deploy/docker/Caddyfile.example",
        (
            "admin off",
            "http_port 8080",
            "https_port 8443",
            "reverse_proxy xs-nexus-dev-console:8080",
            "Strict-Transport-Security",
        ),
    )
    if "tls internal" in caddy:
        raise SystemExit("public Edge template must use publicly trusted ACME certificates")

    print("public HTTPS Edge source validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
