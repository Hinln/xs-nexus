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
            "deploy/docker/edge.Dockerfile",
        ),
    )
    if "privileged:" in compose or "network_mode: host" in compose:
        raise SystemExit("edge compose must not use privileged or host networking")

    edge = require(
        ROOT / "deploy/docker/xs-nexus-edge.sh",
        (
            "edge image must use an isolated XS Nexus tag",
            "edge image revision does not match the deployment revision",
            "validate_private_path",
            "require_application",
            "validate_available_port",
            "--pull never",
            "caddy reload",
        ),
    )
    if "docker compose down" in edge or "docker system prune" in edge:
        raise SystemExit("edge lifecycle must not tear down unrelated resources")

    dockerfile = require(
        ROOT / "deploy/docker/edge.Dockerfile",
        (
            "golang:1.26.5-alpine3.23@sha256:622e56dbc11a8cfe87cafa2331e9a201877271cbff918af53d3be315f3da88cc",
            "alpine:3.23@sha256:fd791d74b68913cbb027c6546007b3f0d3bc45125f797758156952bc2d6daf40",
            "CADDY_REVISION=e2eee6a7fce366321294c9c2a79f3146891dcbdf",
            "CADDY_CEL_PATCH_REVISION=b2693fb63a30e6d7be0972c3645e9a2c0a500e93",
            'github.com/google/cel-go)" = "v0.29.2"',
            "go.opentelemetry.io/otel@v1.44.0",
            "golang.org/x/text@v0.39.0",
            "google.golang.org/grpc@v1.82.1",
            "CustomVersion=v2.11.4-xs1",
            "setcap -r /usr/bin/caddy",
            "spdx-licenses-text",
            "USER 65532:65532",
            'org.opencontainers.image.revision="$VCS_REF"',
            'org.opencontainers.image.version="$XS_VERSION"',
            'org.opencontainers.image.source="$XS_SOURCE_URL"',
        ),
    )
    if "latest" in dockerfile:
        raise SystemExit("edge base image must not use latest")

    stack = require(
        ROOT / "deploy/docker/xs-nexus-stack.sh",
        (
            "XS_BIND_ADDRESS",
            "XS_UDP_BIND_ADDRESS",
            "HTTP services must bind to 127.0.0.1",
            "UDP services must bind to 127.0.0.1 or 0.0.0.0",
            "label=com.docker.compose.service",
            "port is already in use",
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

    openresty = require(
        ROOT / "deploy/host/openresty-xs-nexus-vhost.conf.example",
        (
            "server_name __XS_NEXUS_DOMAIN__;",
            "ssl_certificate __XS_NEXUS_CERTIFICATE_FULLCHAIN__;",
            "ssl_certificate_key __XS_NEXUS_CERTIFICATE_KEY__;",
            "ssl_protocols TLSv1.2 TLSv1.3;",
            "ssl_session_tickets off;",
            "return 308 https://$host$request_uri;",
            "proxy_pass http://127.0.0.1:28081;",
            "proxy_http_version 1.1;",
            "proxy_set_header Upgrade $http_upgrade;",
            "proxy_set_header Connection $xs_nexus_connection_upgrade;",
            "Strict-Transport-Security",
        ),
    )
    for forbidden in (
        "proxy_ssl_verify off",
        "ssl_protocols TLSv1 ",
        "ssl_protocols TLSv1.1",
        "127.0.0.1:28080",
    ):
        if forbidden in openresty:
            raise SystemExit(f"OpenResty planned-domain template contains forbidden setting {forbidden!r}")
    if openresty.count("proxy_pass http://127.0.0.1:28081;") != 1:
        raise SystemExit("OpenResty planned-domain template must have one Console upstream")

    print("public HTTPS Edge source validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
