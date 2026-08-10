#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/relay-throughput-$(date -u +%Y%m%dT%H%M%SZ)"}
PACKETS=${XS_RELAY_THROUGHPUT_PACKETS:-1000000}
MINIMUM_PACKETS_PER_SECOND=${XS_RELAY_MINIMUM_PACKETS_PER_SECOND:-10000}
[[ $PACKETS =~ ^[0-9]+$ ]]
[[ $MINIMUM_PACKETS_PER_SECOND =~ ^[0-9]+$ ]]
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
cd "$ROOT_DIR"
git rev-parse HEAD >"$EVIDENCE_DIR/revision.txt"
{
    rustc --version
    cargo --version
    uname -a
} >"$EVIDENCE_DIR/environment.txt"
XS_RELAY_THROUGHPUT_PACKETS="$PACKETS" \
    XS_RELAY_THROUGHPUT_REPORT="$EVIDENCE_DIR/report.json" \
    cargo test --release -p xs-relay --lib udp_relay_throughput_baseline -- --nocapture \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
test -s "$EVIDENCE_DIR/report.json"
python3 - "$EVIDENCE_DIR/report.json" "$PACKETS" "$MINIMUM_PACKETS_PER_SECOND" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_packets = int(sys.argv[2])
minimum_packets_per_second = int(sys.argv[3])
assert report["packets"] == expected_packets
assert report["packets_per_second"] >= minimum_packets_per_second
assert report["elapsed_ms"] > 0
assert report["forwarding_latency_microseconds_average"] is not None
PY
sha256sum \
    "$EVIDENCE_DIR/environment.txt" \
    "$EVIDENCE_DIR/report.json" \
    "$EVIDENCE_DIR/revision.txt" \
    "$EVIDENCE_DIR/test-output.txt" \
    >"$EVIDENCE_DIR/SHA256SUMS"
printf 'Relay throughput baseline passed; evidence: %s\n' "$EVIDENCE_DIR"
