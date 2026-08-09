#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import hashlib
import ipaddress
import json
import os
import re
import socket
import ssl
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


MAX_HTTP_HEAD_BYTES = 64 * 1024
WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
HTTP_STATUS_RE = re.compile(r"^HTTP/1\.[01] ([1-5][0-9]{2})(?:[ \t].*)?$")


class AuditError(RuntimeError):
    pass


@dataclass(frozen=True)
class EndpointCheck:
    path: str
    expected_status: int
    websocket: bool


def validate_hostname(value: str) -> str:
    if not value or len(value) > 253:
        raise argparse.ArgumentTypeError("hostname must contain 1 to 253 characters")
    if any(character.isspace() or ord(character) < 32 for character in value):
        raise argparse.ArgumentTypeError("hostname must not contain whitespace or controls")
    if any(character in value for character in "/\\@"):
        raise argparse.ArgumentTypeError("hostname must not contain paths or user information")
    hostname = value[1:-1] if value.startswith("[") and value.endswith("]") else value
    try:
        return str(ipaddress.ip_address(hostname))
    except ValueError:
        pass
    if ":" in hostname:
        raise argparse.ArgumentTypeError("hostname must not include a port")
    try:
        ascii_hostname = hostname.encode("idna").decode("ascii")
    except UnicodeError as error:
        raise argparse.ArgumentTypeError("hostname is not valid IDNA") from error
    labels = ascii_hostname[:-1].split(".") if ascii_hostname.endswith(".") else ascii_hostname.split(".")
    if any(
        not label
        or len(label) > 63
        or label.startswith("-")
        or label.endswith("-")
        or not all(character.isalnum() or character == "-" for character in label)
        for label in labels
    ):
        raise argparse.ArgumentTypeError("hostname contains an invalid DNS label")
    return ascii_hostname


def validate_path(value: str) -> str:
    if not value.startswith("/") or len(value) > 2048:
        raise argparse.ArgumentTypeError("request path must start with / and be at most 2048 characters")
    if any(ord(character) < 33 or ord(character) == 127 for character in value):
        raise argparse.ArgumentTypeError("request path must not contain control characters")
    try:
        value.encode("ascii")
    except UnicodeEncodeError as error:
        raise argparse.ArgumentTypeError("request path must be ASCII or percent-encoded") from error
    return value


def parse_http_check(value: str) -> EndpointCheck:
    try:
        path, status_text = value.rsplit("=", 1)
        expected_status = int(status_text)
    except (ValueError, TypeError) as error:
        raise argparse.ArgumentTypeError("HTTP check must use PATH=STATUS") from error
    if expected_status < 100 or expected_status > 599:
        raise argparse.ArgumentTypeError("HTTP status must be between 100 and 599")
    return EndpointCheck(validate_path(path), expected_status, False)


def parse_websocket_check(value: str) -> EndpointCheck:
    return EndpointCheck(validate_path(value), 101, True)


def sanitized_error(error: BaseException) -> dict[str, str]:
    if isinstance(error, ssl.SSLCertVerificationError):
        code = "certificate_verification_failed"
    elif isinstance(error, ssl.SSLError):
        code = "tls_failed"
    elif isinstance(error, socket.timeout):
        code = "timeout"
    elif isinstance(error, socket.gaierror):
        code = "dns_failed"
    elif isinstance(error, AuditError):
        code = "protocol_failed"
    elif isinstance(error, OSError):
        code = "network_failed"
    else:
        code = "unexpected_failure"
    message = " ".join(str(error).replace("\x00", "").split())[:500]
    return {"code": code, "message": message or error.__class__.__name__}


def flatten_distinguished_name(value: Any) -> list[dict[str, str]]:
    result: list[dict[str, str]] = []
    if not isinstance(value, (tuple, list)):
        return result
    for relative_name in value:
        if not isinstance(relative_name, (tuple, list)):
            continue
        for attribute in relative_name:
            if (
                isinstance(attribute, (tuple, list))
                and len(attribute) == 2
                and isinstance(attribute[0], str)
                and isinstance(attribute[1], str)
            ):
                result.append({"name": attribute[0], "value": attribute[1]})
    return result


def certificate_metadata(tls_socket: ssl.SSLSocket) -> dict[str, Any]:
    certificate = tls_socket.getpeercert()
    certificate_der = tls_socket.getpeercert(binary_form=True)
    if not isinstance(certificate, dict):
        raise AuditError("verified peer certificate metadata is unavailable")
    if not isinstance(certificate_der, bytes) or not certificate_der:
        raise AuditError("verified peer certificate DER is unavailable")
    subject_alt_names = []
    for entry in certificate.get("subjectAltName", ()):
        if isinstance(entry, (tuple, list)) and len(entry) == 2:
            subject_alt_names.append({"type": str(entry[0]), "value": str(entry[1])})
    return {
        "sha256": hashlib.sha256(certificate_der or b"").hexdigest(),
        "subject": flatten_distinguished_name(certificate.get("subject", ())),
        "issuer": flatten_distinguished_name(certificate.get("issuer", ())),
        "subject_alt_names": subject_alt_names,
        "not_before": certificate.get("notBefore"),
        "not_after": certificate.get("notAfter"),
    }


def open_strict_tls(
    connect_host: str,
    port: int,
    server_hostname: str,
    timeout: float,
    ca_file: str | None,
) -> tuple[ssl.SSLSocket, dict[str, Any]]:
    context = ssl.create_default_context(cafile=ca_file)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.check_hostname = True
    context.verify_mode = ssl.CERT_REQUIRED
    raw_socket = socket.create_connection((connect_host, port), timeout=timeout)
    try:
        tls_socket = context.wrap_socket(raw_socket, server_hostname=server_hostname)
    except Exception:
        raw_socket.close()
        raise
    try:
        negotiated_version = tls_socket.version()
        if negotiated_version not in {"TLSv1.2", "TLSv1.3"}:
            raise AuditError(f"negotiated disallowed TLS version {negotiated_version!r}")
        cipher = tls_socket.cipher()
        metadata = {
            "status": "PASS",
            "sni": server_hostname,
            "verification": "CERT_REQUIRED_AND_HOSTNAME",
            "minimum_version": "TLSv1.2",
            "negotiated_version": negotiated_version,
            "cipher": cipher[0] if cipher else None,
            "certificate": certificate_metadata(tls_socket),
        }
    except Exception:
        tls_socket.close()
        raise
    return tls_socket, metadata


def host_header(hostname: str, port: int) -> str:
    rendered = f"[{hostname}]" if ":" in hostname else hostname
    return rendered if port == 443 else f"{rendered}:{port}"


def build_request(check: EndpointCheck, hostname: str, port: int) -> tuple[bytes, str | None]:
    headers = [
        f"GET {check.path} HTTP/1.1",
        f"Host: {host_header(hostname, port)}",
        "User-Agent: xs-nexus-strict-tls-audit/1",
        "Accept: */*",
        "Connection: close",
    ]
    websocket_key = None
    if check.websocket:
        websocket_key = base64.b64encode(os.urandom(16)).decode("ascii")
        headers[-1] = "Connection: Upgrade"
        headers.extend(
            (
                "Upgrade: websocket",
                f"Sec-WebSocket-Key: {websocket_key}",
                "Sec-WebSocket-Version: 13",
            )
        )
    return ("\r\n".join(headers) + "\r\n\r\n").encode("ascii"), websocket_key


def read_http_head(tls_socket: ssl.SSLSocket) -> tuple[int, dict[str, str]]:
    response = bytearray()
    while b"\r\n\r\n" not in response:
        if len(response) >= MAX_HTTP_HEAD_BYTES:
            raise AuditError("HTTP response headers exceed 65536 bytes")
        chunk = tls_socket.recv(min(4096, MAX_HTTP_HEAD_BYTES - len(response)))
        if not chunk:
            raise AuditError("connection closed before complete HTTP response headers")
        response.extend(chunk)
    raw_head = bytes(response).split(b"\r\n\r\n", 1)[0]
    try:
        lines = raw_head.decode("iso-8859-1").split("\r\n")
    except UnicodeDecodeError as error:
        raise AuditError("HTTP response headers are not decodable") from error
    status_match = HTTP_STATUS_RE.fullmatch(lines[0])
    if not status_match:
        raise AuditError("invalid HTTP status line")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if not line or line[0] in " \t" or ":" not in line:
            raise AuditError("invalid HTTP response header")
        name, value = line.split(":", 1)
        normalized_name = name.strip().lower()
        if not normalized_name:
            raise AuditError("empty HTTP response header name")
        normalized_value = value.strip()
        headers[normalized_name] = (
            f"{headers[normalized_name]}, {normalized_value}"
            if normalized_name in headers
            else normalized_value
        )
    return int(status_match.group(1)), headers


def validate_websocket_response(headers: dict[str, str], websocket_key: str) -> None:
    if headers.get("upgrade", "").lower() != "websocket":
        raise AuditError("WebSocket response is missing Upgrade: websocket")
    connection_tokens = {token.strip().lower() for token in headers.get("connection", "").split(",")}
    if "upgrade" not in connection_tokens:
        raise AuditError("WebSocket response is missing Connection: upgrade")
    expected_accept = base64.b64encode(
        hashlib.sha1((websocket_key + WEBSOCKET_GUID).encode("ascii"), usedforsecurity=False).digest()
    ).decode("ascii")
    if headers.get("sec-websocket-accept") != expected_accept:
        raise AuditError("WebSocket response has an invalid Sec-WebSocket-Accept")


def probe_endpoint(
    connect_host: str,
    port: int,
    server_hostname: str,
    timeout: float,
    ca_file: str | None,
    check: EndpointCheck,
) -> dict[str, Any]:
    result: dict[str, Any] = {
        "kind": "websocket" if check.websocket else "http",
        "path": check.path,
        "expected_status": check.expected_status,
        "status": "FAIL",
    }
    tls_socket: ssl.SSLSocket | None = None
    try:
        tls_socket, tls_metadata = open_strict_tls(
            connect_host,
            port,
            server_hostname,
            timeout,
            ca_file,
        )
        result["tls"] = tls_metadata
        request, websocket_key = build_request(check, server_hostname, port)
        tls_socket.sendall(request)
        actual_status, headers = read_http_head(tls_socket)
        result["actual_status"] = actual_status
        result["response_headers"] = {
            name: headers[name]
            for name in ("server", "content-type", "upgrade", "connection")
            if name in headers
        }
        if actual_status != check.expected_status:
            raise AuditError(
                f"expected HTTP {check.expected_status} for {check.path}, received {actual_status}"
            )
        if check.websocket:
            if websocket_key is None:
                raise AuditError("WebSocket audit request key was not generated")
            validate_websocket_response(headers, websocket_key)
        result["status"] = "PASS"
    except Exception as error:
        result["error"] = sanitized_error(error)
    finally:
        if tls_socket is not None:
            tls_socket.close()
    return result


def probe_tls_only(
    connect_host: str,
    port: int,
    server_hostname: str,
    timeout: float,
    ca_file: str | None,
) -> dict[str, Any]:
    result: dict[str, Any] = {"kind": "tls", "status": "FAIL"}
    tls_socket: ssl.SSLSocket | None = None
    try:
        tls_socket, tls_metadata = open_strict_tls(
            connect_host,
            port,
            server_hostname,
            timeout,
            ca_file,
        )
        result["tls"] = tls_metadata
        result["status"] = "PASS"
    except Exception as error:
        result["error"] = sanitized_error(error)
    finally:
        if tls_socket is not None:
            tls_socket.close()
    return result


def resolve_addresses(hostname: str, port: int) -> dict[str, Any]:
    try:
        addresses = sorted(
            {
                item[4][0]
                for item in socket.getaddrinfo(hostname, port, type=socket.SOCK_STREAM)
            }
        )
        if not addresses:
            raise AuditError("resolver returned no addresses")
        return {"status": "PASS", "addresses": addresses}
    except Exception as error:
        return {"status": "FAIL", "error": sanitized_error(error)}


def audit_target(
    target_id: str,
    connect_host: str,
    port: int,
    server_hostname: str,
    timeout: float,
    ca_file: str | None,
    checks: list[EndpointCheck],
) -> dict[str, Any]:
    resolution = resolve_addresses(connect_host, port)
    results = [
        probe_endpoint(connect_host, port, server_hostname, timeout, ca_file, check)
        for check in checks
    ]
    if not results:
        results.append(probe_tls_only(connect_host, port, server_hostname, timeout, ca_file))
    status = (
        "PASS"
        if resolution["status"] == "PASS" and all(result["status"] == "PASS" for result in results)
        else "FAIL"
    )
    return {
        "target_id": target_id,
        "connect_host": connect_host,
        "port": port,
        "sni": server_hostname,
        "resolution": resolution,
        "checks": results,
        "status": status,
    }


def build_report(arguments: argparse.Namespace) -> dict[str, Any]:
    checks = [*arguments.http_check, *arguments.websocket_check]
    targets = [
        audit_target(
            "edge",
            arguments.hostname,
            arguments.port,
            arguments.hostname,
            arguments.timeout,
            arguments.ca_file,
            checks,
        )
    ]
    if arguments.origin:
        targets.append(
            audit_target(
                "origin",
                arguments.origin,
                arguments.port,
                arguments.hostname,
                arguments.timeout,
                arguments.ca_file,
                checks,
            )
        )
    overall_status = "PASS" if all(target["status"] == "PASS" for target in targets) else "FAIL"
    return {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
        "hostname": arguments.hostname,
        "origin_requested": arguments.origin is not None,
        "overall_status": overall_status,
        "targets": targets,
        "limitations": [
            "This report observes strict certificate and hostname verification from the audit client only.",
            (
                "It does not prove the CDN control-plane TLS mode, browser UI behavior, "
                "or production-gate completion."
            ),
        ],
    }


def write_report(path: Path | None, report: dict[str, Any]) -> None:
    rendered = json.dumps(report, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    if path is None:
        print(rendered, end="")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(rendered, encoding="utf-8")
    os.replace(temporary, path)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Audit edge and direct-origin strict TLS, HTTP, and WebSocket behavior."
    )
    parser.add_argument("--hostname", required=True, type=validate_hostname)
    parser.add_argument("--origin", type=validate_hostname)
    parser.add_argument("--port", type=int, default=443)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--ca-file")
    parser.add_argument("--http-check", action="append", type=parse_http_check, default=[])
    parser.add_argument("--websocket-check", action="append", type=parse_websocket_check, default=[])
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    if arguments.port < 1 or arguments.port > 65535:
        parser.error("port must be between 1 and 65535")
    if arguments.timeout <= 0 or arguments.timeout > 120:
        parser.error("timeout must be greater than zero and at most 120 seconds")
    if arguments.ca_file and not Path(arguments.ca_file).is_file():
        parser.error("CA file does not exist or is not a regular file")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    report = build_report(arguments)
    write_report(arguments.output, report)
    return 0 if report["overall_status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
