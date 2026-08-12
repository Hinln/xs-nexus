#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly ROOT_DIR
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/protocol-throughput-$(date -u +%Y%m%dT%H%M%SZ)"}
readonly MINIMUM_OPERATIONS_PER_SECOND=20000
readonly MINIMUM_MIB_PER_SECOND=20
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
EVIDENCE_DIR=$(cd "$EVIDENCE_DIR" && pwd -P)
readonly EVIDENCE_DIR
cd "$ROOT_DIR"
revision=$(git rev-parse HEAD)
readonly revision
printf '%s\n' "$revision" >"$EVIDENCE_DIR/revision.txt"
printf 'status=FAIL\nrevision=%s\n' "$revision" >"$EVIDENCE_DIR/status.txt"
printf 'cargo test --release -p xs-protocol --lib encryption_throughput_baseline -- --nocapture\n' \
    >"$EVIDENCE_DIR/command.txt"
{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uname -a
    rustc --version --verbose
    cargo --version
    python3 --version
} >"$EVIDENCE_DIR/environment.txt" 2>&1
start_epoch=$(date +%s)
set +e
XS_PROTOCOL_THROUGHPUT_REPORT="$EVIDENCE_DIR/report.json" \
    cargo test --release -p xs-protocol --lib encryption_throughput_baseline -- --nocapture \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
test_status=$?
set -e
printf '%s\n' "$(( $(date +%s) - start_epoch ))" >"$EVIDENCE_DIR/elapsed-seconds.txt"
((test_status == 0)) || exit "$test_status"
test -s "$EVIDENCE_DIR/report.json"
python3 - \
    "$EVIDENCE_DIR/report.json" \
    "$revision" \
    "$MINIMUM_OPERATIONS_PER_SECOND" \
    "$MINIMUM_MIB_PER_SECOND" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
report = json.loads(path.read_text(encoding="utf-8"))
minimum_operations_per_second = int(sys.argv[3])
minimum_mib_per_second = int(sys.argv[4])
assert report["iterations"] == 50_000
assert report["packet_bytes"] == 1_200
assert report["plaintext_bytes"] == 60_000_000
assert report["elapsed_ms"] > 0
assert report["encrypt_decrypt_ops_per_second"] >= minimum_operations_per_second
assert report["plaintext_mib_per_second"] >= minimum_mib_per_second
report["schema_version"] = 1
report["git_revision"] = sys.argv[2]
report["safe_operating_envelope"] = {
    "minimum_encrypt_decrypt_ops_per_second": minimum_operations_per_second,
    "minimum_plaintext_mib_per_second": minimum_mib_per_second,
    "packet_bytes": 1_200,
}
path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
secret_log=$(mktemp)
python3 scripts/check-secrets.py --root "$EVIDENCE_DIR" >"$secret_log" 2>&1
mv -- "$secret_log" "$EVIDENCE_DIR/secret-scan.log"
printf 'status=PASS\nrevision=%s\n' "$revision" >"$EVIDENCE_DIR/status.txt"
checksum_file=$(mktemp)
(
    cd "$EVIDENCE_DIR"
    sha256sum \
        command.txt \
        elapsed-seconds.txt \
        environment.txt \
        report.json \
        revision.txt \
        secret-scan.log \
        status.txt \
        test-output.txt
) >"$checksum_file"
mv -- "$checksum_file" "$EVIDENCE_DIR/SHA256SUMS"
printf 'protocol throughput baseline passed; evidence: %s\n' "$EVIDENCE_DIR"
