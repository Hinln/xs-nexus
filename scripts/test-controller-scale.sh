#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly ROOT_DIR
readonly EXTERNAL_ENVIRONMENT=/etc/xs-nexus/controller.env
readonly SCHEMA=xs_nexus_scale
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/controller-scale-$(date -u +%Y%m%dT%H%M%SZ)"}
SCHEMA_CLEAN=0

load_database_url() {
    if [[ -n ${XS_TEST_DATABASE_URL:-} ]]; then
        printf '%s\n' "$XS_TEST_DATABASE_URL"
        return
    fi
    [[ -r $EXTERNAL_ENVIRONMENT ]]
    python3 - "$EXTERNAL_ENVIRONMENT" <<'PY'
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit
import sys

values = {}
for raw_line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    line = raw_line.strip()
    if line and not line.startswith("#") and "=" in line:
        name, value = line.split("=", 1)
        values[name] = value

parsed = urlsplit(values["DATABASE_URL"])
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, "", "")))
PY
}

cleanup_schema() {
    (
        cd "$ROOT_DIR"
        DATABASE_URL=$XS_TEST_DATABASE_URL DATABASE_SCHEMA=$SCHEMA \
            cargo run --quiet --release -p xs-controller --example reset_test_schema >/dev/null
    )
}

cleanup() {
    local status=$1
    local cleanup_status=0
    trap - EXIT INT TERM
    set +e
    if ((SCHEMA_CLEAN == 0)); then
        cleanup_schema || cleanup_status=$?
    fi
    if ((status == 0 && cleanup_status != 0)); then
        status=$cleanup_status
    fi
    exit "$status"
}

mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
EVIDENCE_DIR=$(cd "$EVIDENCE_DIR" && pwd -P)
readonly EVIDENCE_DIR
XS_TEST_DATABASE_URL=$(load_database_url)
export XS_TEST_DATABASE_URL
export XS_TEST_DATABASE_SCHEMA=$SCHEMA
export XS_SCALE_REPORT_PATH="$EVIDENCE_DIR/report.json"

cd "$ROOT_DIR"
trap 'cleanup "$?"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
revision=$(git rev-parse HEAD)
readonly revision
export XS_SCALE_GIT_REVISION=$revision
printf '%s\n' "$revision" >"$EVIDENCE_DIR/revision.txt"
printf 'status=FAIL\nrevision=%s\n' "$revision" >"$EVIDENCE_DIR/status.txt"
printf 'cargo test --release -p xs-controller --test controller_scale -- --test-threads=1 --nocapture\n' \
    >"$EVIDENCE_DIR/command.txt"
{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uname -a
    rustc --version --verbose
    cargo --version
    python3 --version
} >"$EVIDENCE_DIR/environment.txt" 2>&1

cleanup_schema
start_epoch=$(date +%s)
set +e
cargo test --release -p xs-controller --test controller_scale -- --test-threads=1 --nocapture \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
test_status=$?
set -e
printf '%s\n' "$(( $(date +%s) - start_epoch ))" >"$EVIDENCE_DIR/elapsed-seconds.txt"
((test_status == 0)) || exit "$test_status"
test -s "$EVIDENCE_DIR/report.json"

python3 - "$EVIDENCE_DIR/report.json" "$revision" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert report["git_revision"] == sys.argv[2]
assert report["registration_concurrency"] == 32
assert report["token_count"] == 20
assert [item["nodes"] for item in report["measurements"]] == [100, 500, 1000]
envelope = report["safe_operating_envelope"]
for measurement in report["measurements"]:
    assert measurement["active_nodes"] == measurement["nodes"]
    assert measurement["distinct_addresses"] == measurement["nodes"]
    assert measurement["batch_registrations_per_second"] >= envelope[
        "minimum_registrations_per_second"
    ]
    assert measurement["console_snapshot_ms"] <= envelope[
        "maximum_console_snapshot_milliseconds"
    ]
    assert measurement["database_count_query_ms"] <= envelope[
        "maximum_database_query_milliseconds"
    ]
control = report["control"]
assert control["connections"] == 1000
assert control["authentication_concurrency"] == 64
assert control["synchronization_concurrency"] == 128
assert control["authentications_per_second"] >= envelope[
    "minimum_control_authentications_per_second"
]
assert control["authentication_p95_ms"] <= envelope[
    "maximum_control_authentication_p95_milliseconds"
]
assert control["synchronization_p95_ms"] <= envelope[
    "maximum_control_sync_p95_milliseconds"
]
assert control["console_snapshot_ms"] <= envelope[
    "maximum_console_snapshot_milliseconds"
]
assert control["process_rss_kib"] <= envelope["maximum_process_rss_kib"]
assert control["process_file_descriptors"] <= envelope[
    "maximum_process_file_descriptors"
]
assert control["database_pool_connections"] <= envelope[
    "maximum_database_pool_connections"
]
assert control["online_convergence_ms"] <= envelope[
    "maximum_presence_convergence_milliseconds"
]
assert control["offline_convergence_ms"] <= envelope[
    "maximum_presence_convergence_milliseconds"
]
assert control["hold_seconds"] == 5
PY

cleanup_schema
SCHEMA_CLEAN=1
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
printf 'controller scale test passed; evidence: %s\n' "$EVIDENCE_DIR"
