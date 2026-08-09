#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import hashlib
import importlib.util
import ssl
import sys
from pathlib import Path
from typing import Any
from unittest import mock


SCRIPT = Path(__file__).with_name("audit-strict-tls.py")
SPEC = importlib.util.spec_from_file_location("audit_strict_tls", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load strict TLS audit module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class FakeRawSocket:
    def __init__(self) -> None:
        self.closed = False

    def close(self) -> None:
        self.closed = True


class FakeTlsSocket:
    def __init__(self) -> None:
        self.response = b""
        self.closed = False

    def version(self) -> str:
        return "TLSv1.3"

    def cipher(self) -> tuple[str, str, int]:
        return ("TLS_AES_256_GCM_SHA384", "TLSv1.3", 256)

    def getpeercert(self, binary_form: bool = False) -> Any:
        if binary_form:
            return b"strict-tls-fixture-certificate"
        return {
            "subject": ((('commonName', 'vpn.example.test'),),),
            "issuer": ((('commonName', 'Fixture CA'),),),
            "subjectAltName": (("DNS", "vpn.example.test"),),
            "notBefore": "Jan 1 00:00:00 2026 GMT",
            "notAfter": "Jan 1 00:00:00 2030 GMT",
        }

    def sendall(self, request: bytes) -> None:
        request_text = request.decode("ascii")
        if "Upgrade: websocket\r\n" in request_text:
            key_line = next(
                line for line in request_text.split("\r\n") if line.startswith("Sec-WebSocket-Key: ")
            )
            websocket_key = key_line.split(": ", 1)[1]
            accept = base64.b64encode(
                hashlib.sha1(
                    (websocket_key + MODULE.WEBSOCKET_GUID).encode("ascii"),
                    usedforsecurity=False,
                ).digest()
            ).decode("ascii")
            self.response = (
                "HTTP/1.1 101 Switching Protocols\r\n"
                "Upgrade: websocket\r\n"
                "Connection: keep-alive, Upgrade\r\n"
                f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
            ).encode("ascii")
        else:
            self.response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n"

    def recv(self, size: int) -> bytes:
        chunk = self.response[:size]
        self.response = self.response[size:]
        return chunk

    def close(self) -> None:
        self.closed = True


class FakeTlsContext:
    def __init__(self, fail_verification: bool = False) -> None:
        self.minimum_version = None
        self.check_hostname = False
        self.verify_mode = None
        self.fail_verification = fail_verification
        self.server_hostnames: list[str] = []

    def wrap_socket(self, raw_socket: FakeRawSocket, server_hostname: str) -> FakeTlsSocket:
        self.server_hostnames.append(server_hostname)
        if self.fail_verification:
            raise ssl.SSLCertVerificationError(1, "certificate verify failed")
        return FakeTlsSocket()


def expect_argument_failure(function: Any, value: str) -> None:
    try:
        function(value)
    except argparse.ArgumentTypeError:
        return
    raise RuntimeError(f"invalid argument was accepted: {value!r}")


def main() -> int:
    http_check = MODULE.parse_http_check("/health/ready=200")
    websocket_check = MODULE.parse_websocket_check("/v1/control")
    if http_check.expected_status != 200 or http_check.websocket:
        raise RuntimeError("HTTP check parser returned the wrong result")
    if websocket_check.expected_status != 101 or not websocket_check.websocket:
        raise RuntimeError("WebSocket check parser returned the wrong result")
    expect_argument_failure(MODULE.parse_http_check, "/health/ready")
    expect_argument_failure(MODULE.parse_http_check, "/health/ready=700")
    expect_argument_failure(MODULE.parse_http_check, "/health ready=200")
    expect_argument_failure(MODULE.parse_websocket_check, "https://example.test/")
    expect_argument_failure(MODULE.validate_hostname, "user@example.test")
    expect_argument_failure(MODULE.validate_hostname, "vpn.example.test:443")

    contexts: list[FakeTlsContext] = []
    connections: list[tuple[tuple[str, int], float]] = []

    def context_factory(cafile: str | None = None) -> FakeTlsContext:
        if cafile is not None:
            raise RuntimeError("unexpected CA fixture")
        context = FakeTlsContext()
        contexts.append(context)
        return context

    def connection_factory(address: tuple[str, int], timeout: float) -> FakeRawSocket:
        connections.append((address, timeout))
        return FakeRawSocket()

    with (
        mock.patch.object(MODULE.ssl, "create_default_context", side_effect=context_factory),
        mock.patch.object(MODULE.socket, "create_connection", side_effect=connection_factory),
        mock.patch.object(
            MODULE.socket,
            "getaddrinfo",
            return_value=[(2, 1, 6, "", ("203.0.113.10", 443))],
        ),
    ):
        target = MODULE.audit_target(
            "origin",
            "203.0.113.10",
            443,
            "vpn.example.test",
            3.0,
            None,
            [http_check, websocket_check],
        )

    if target["status"] != "PASS":
        raise RuntimeError("valid strict TLS target did not pass")
    if len(target["checks"]) != 2 or any(check["status"] != "PASS" for check in target["checks"]):
        raise RuntimeError("endpoint checks did not pass")
    if connections != [(('203.0.113.10', 443), 3.0), (('203.0.113.10', 443), 3.0)]:
        raise RuntimeError("origin connection target was not preserved")
    if any(context.server_hostnames != ["vpn.example.test"] for context in contexts):
        raise RuntimeError("origin connection did not use the planned-domain SNI")
    if any(context.minimum_version != ssl.TLSVersion.TLSv1_2 for context in contexts):
        raise RuntimeError("minimum TLS version was not enforced")
    if any(not context.check_hostname or context.verify_mode != ssl.CERT_REQUIRED for context in contexts):
        raise RuntimeError("strict certificate and hostname verification was not enforced")

    failing_context = FakeTlsContext(fail_verification=True)
    with (
        mock.patch.object(MODULE.ssl, "create_default_context", return_value=failing_context),
        mock.patch.object(MODULE.socket, "create_connection", return_value=FakeRawSocket()),
    ):
        failed = MODULE.probe_endpoint(
            "203.0.113.10",
            443,
            "vpn.example.test",
            3.0,
            None,
            http_check,
        )
    if failed["status"] != "FAIL":
        raise RuntimeError("certificate verification failure did not fail closed")
    if failed["error"]["code"] != "certificate_verification_failed":
        raise RuntimeError("certificate verification failure was misclassified")

    malformed = FakeTlsSocket()
    malformed.response = b"HTTP/1.1 200 OK\r\n folded: forbidden\r\n\r\n"
    try:
        MODULE.read_http_head(malformed)
    except MODULE.AuditError:
        pass
    else:
        raise RuntimeError("folded HTTP response header was accepted")

    print("strict TLS audit regression tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
