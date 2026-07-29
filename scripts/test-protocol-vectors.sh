#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

generated="$(mktemp)"
session_generated="$(mktemp)"
discovery_generated="$(mktemp)"
trap 'rm -f "$generated" "$session_generated" "$discovery_generated"' EXIT

cargo run --quiet -p xs-protocol --example generate_credential_vector >"$generated"
if ! cmp --silent "$generated" tests/vectors/xsp1/credential-v1.json; then
    diff -u tests/vectors/xsp1/credential-v1.json "$generated" || true
    printf 'credential vector differs from the checked-in file\n' >&2
    exit 1
fi

printf 'XSP/1 credential vector generation passed\n'

cargo run --quiet -p xs-protocol --example generate_session_vector >"$session_generated"
if ! cmp --silent "$session_generated" tests/vectors/xsp1/session-v1.json; then
    diff -u tests/vectors/xsp1/session-v1.json "$session_generated" || true
    printf 'session vector differs from the checked-in file\n' >&2
    exit 1
fi

python3 - "$session_generated" <<'PY'
import json
import sys
from pathlib import Path

vector = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))

def changed(original, index, *, value=None):
    malformed = bytearray(original)
    malformed[index] = value if value is not None else malformed[index] ^ 1
    return bytes(malformed)

client_hello = bytes.fromhex(vector["client_hello_hex"])
server_finish = bytes.fromhex(vector["server_finish_hex"])
data_packet = bytes.fromhex(vector["data_packet_hex"])
corpora = {
    "fuzz/corpus/handshake/client-hello-v1.bin": client_hello,
    "fuzz/corpus/handshake/server-hello-v1.bin": bytes.fromhex(vector["server_hello_hex"]),
    "fuzz/corpus/handshake/client-finish-v1.bin": bytes.fromhex(vector["client_finish_hex"]),
    "fuzz/corpus/handshake/server-finish-v1.bin": server_finish,
    "fuzz/corpus/handshake/client-hello-invalid-magic-v1.bin": changed(client_hello, 0),
    "fuzz/corpus/handshake/client-hello-invalid-version-v1.bin": changed(client_hello, 4, value=2),
    "fuzz/corpus/handshake/client-hello-invalid-type-v1.bin": changed(client_hello, 5, value=0x11),
    "fuzz/corpus/handshake/client-hello-invalid-flags-v1.bin": changed(client_hello, 6, value=1),
    "fuzz/corpus/handshake/client-hello-invalid-length-v1.bin": changed(client_hello, 11),
    "fuzz/corpus/handshake/client-hello-truncated-v1.bin": client_hello[:-1],
    "fuzz/corpus/handshake/client-hello-trailing-v1.bin": client_hello + b"\x00",
    "fuzz/corpus/handshake/unexpected-server-finish-v1.bin": server_finish,
    "fuzz/corpus/data/valid-v1.bin": data_packet,
    "fuzz/corpus/data/invalid-magic-v1.bin": changed(data_packet, 0),
    "fuzz/corpus/data/invalid-version-v1.bin": changed(data_packet, 4, value=2),
    "fuzz/corpus/data/invalid-type-v1.bin": changed(data_packet, 5, value=0x10),
    "fuzz/corpus/data/invalid-flags-v1.bin": changed(data_packet, 6, value=0x80),
    "fuzz/corpus/data/invalid-header-length-v1.bin": changed(data_packet, 9),
    "fuzz/corpus/data/invalid-payload-length-v1.bin": changed(data_packet, 11),
    "fuzz/corpus/data/invalid-reserved-v1.bin": changed(data_packet, 95, value=1),
    "fuzz/corpus/data/truncated-v1.bin": data_packet[:-1],
    "fuzz/corpus/data/trailing-v1.bin": data_packet + b"\x00",
}
for relative, expected in corpora.items():
    if Path(relative).read_bytes() != expected:
        raise SystemExit(f"Fuzz corpus mismatch: {relative}")
PY

printf 'XSP/1 session and data vectors passed\n'

cargo run --quiet -p xs-protocol --example generate_discovery_vector >"$discovery_generated"
if ! cmp --silent "$discovery_generated" tests/vectors/xsp1/discovery-v1.json; then
    diff -u tests/vectors/xsp1/discovery-v1.json "$discovery_generated" || true
    printf 'discovery vector differs from the checked-in file\n' >&2
    exit 1
fi

python3 - "$discovery_generated" <<'PY'
import json
import sys
from pathlib import Path

vector = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))

def changed(original, index, *, value=None):
    malformed = bytearray(original)
    malformed[index] = value if value is not None else malformed[index] ^ 1
    return bytes(malformed)

request = bytes.fromhex(vector["request_hex"])
response = bytes.fromhex(vector["response_hex"])
corpora = {
    "fuzz/corpus/discovery/request-v1.bin": request,
    "fuzz/corpus/discovery/response-v1.bin": response,
    "fuzz/corpus/discovery/request-invalid-magic-v1.bin": changed(request, 0),
    "fuzz/corpus/discovery/request-invalid-version-v1.bin": changed(request, 4, value=2),
    "fuzz/corpus/discovery/request-invalid-type-v1.bin": changed(request, 5, value=2),
    "fuzz/corpus/discovery/request-invalid-length-v1.bin": changed(request, 9),
    "fuzz/corpus/discovery/request-truncated-v1.bin": request[:-1],
    "fuzz/corpus/discovery/response-invalid-family-v1.bin": changed(response, 68, value=5),
    "fuzz/corpus/discovery/response-tampered-v1.bin": changed(response, 88),
    "fuzz/corpus/discovery/response-truncated-v1.bin": response[:-1],
}
for relative, expected in corpora.items():
    if Path(relative).read_bytes() != expected:
        raise SystemExit(f"Fuzz corpus mismatch: {relative}")
PY

printf 'authenticated discovery vectors passed\n'
