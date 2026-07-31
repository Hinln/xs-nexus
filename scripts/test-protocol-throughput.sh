#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/protocol-throughput-$(date -u +%Y%m%dT%H%M%SZ)"}
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
cd "$ROOT_DIR"
git rev-parse HEAD >"$EVIDENCE_DIR/revision.txt"
XS_PROTOCOL_THROUGHPUT_REPORT="$EVIDENCE_DIR/report.json" \
    cargo test --release -p xs-protocol --lib encryption_throughput_baseline -- --nocapture \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
test -s "$EVIDENCE_DIR/report.json"
printf 'protocol throughput baseline passed; evidence: %s\n' "$EVIDENCE_DIR"
