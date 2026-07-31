#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EXTERNAL_ENVIRONMENT=/etc/xs-nexus/controller.env
SCHEMA=xs_nexus_scale
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/controller-scale-$(date -u +%Y%m%dT%H%M%SZ)"}

[[ -r $EXTERNAL_ENVIRONMENT ]]
mapfile -t database_settings < <(python3 - "$EXTERNAL_ENVIRONMENT" <<'PY'
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit
import sys

values = {}
for raw_line in Path(sys.argv[1]).read_text().splitlines():
    line = raw_line.strip()
    if line and not line.startswith("#") and "=" in line:
        name, value = line.split("=", 1)
        values[name] = value

parsed = urlsplit(values["DATABASE_URL"])
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)))
PY
)

mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
export XS_TEST_DATABASE_URL=${database_settings[0]}
export XS_TEST_DATABASE_SCHEMA=$SCHEMA
export XS_SCALE_REPORT_PATH="$EVIDENCE_DIR/report.json"

cleanup() {
    DATABASE_URL=$XS_TEST_DATABASE_URL DATABASE_SCHEMA=$SCHEMA \
        cargo run --quiet -p xs-controller --example reset_test_schema >/dev/null || true
}
trap cleanup EXIT INT TERM

cd "$ROOT_DIR"
cleanup
git rev-parse HEAD >"$EVIDENCE_DIR/revision.txt"
start_epoch=$(date +%s)
cargo test -p xs-controller --test controller_scale -- --test-threads=1 --nocapture \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
printf '%s\n' "$(( $(date +%s) - start_epoch ))" >"$EVIDENCE_DIR/elapsed-seconds.txt"
test -s "$EVIDENCE_DIR/report.json"
printf 'controller scale test passed; evidence: %s\n' "$EVIDENCE_DIR"
