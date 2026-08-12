#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly ROOT_DIR
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/agent-rtt-$(date -u +%Y%m%dT%H%M%SZ)"}
readonly MAXIMUM_DIRECT_AVERAGE_MILLISECONDS=5
readonly MAXIMUM_DIRECT_P95_MILLISECONDS=10
readonly MAXIMUM_RELAY_AVERAGE_MILLISECONDS=10
readonly MAXIMUM_RELAY_P95_MILLISECONDS=20
readonly MAXIMUM_RELAY_P95_INCREMENT_MILLISECONDS=15
readonly MAXIMUM_AGENT_RSS_KIB=65536
readonly MAXIMUM_AGENT_IDLE_CPU_PERCENT=5
readonly MAXIMUM_AGENT_FILE_DESCRIPTORS=64
readonly MAXIMUM_AGENT_THREADS=16
readonly EXPECTED_RTT_SAMPLES=100
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
EVIDENCE_DIR=$(cd "$EVIDENCE_DIR" && pwd -P)
readonly EVIDENCE_DIR
cd "$ROOT_DIR"
revision=${XS_EVIDENCE_GIT_REVISION:-$(git rev-parse HEAD)}
[[ $revision =~ ^[0-9a-f]{40}$ ]]
readonly revision
printf '%s\n' "$revision" >"$EVIDENCE_DIR/revision.txt"
printf 'status=FAIL\nrevision=%s\n' "$revision" >"$EVIDENCE_DIR/status.txt"
printf 'XS_AGENT_TEST_PROFILE=release XS_AGENT_RTT_SAMPLE_COUNT=%s RTT_EVIDENCE_DIR=<evidence> ./scripts/test-agent-relay.sh\n' \
    "$EXPECTED_RTT_SAMPLES" \
    >"$EVIDENCE_DIR/command.txt"
{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uname -a
    rustc --version --verbose
    cargo --version
    python3 --version
    ip -Version
} >"$EVIDENCE_DIR/environment.txt" 2>&1
start_epoch=$(date +%s)
set +e
XS_AGENT_TEST_PROFILE=release XS_AGENT_RTT_SAMPLE_COUNT="$EXPECTED_RTT_SAMPLES" \
    RTT_EVIDENCE_DIR="$EVIDENCE_DIR" ./scripts/test-agent-relay.sh \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
test_status=$?
set -e
printf '%s\n' "$(( $(date +%s) - start_epoch ))" >"$EVIDENCE_DIR/elapsed-seconds.txt"
((test_status == 0)) || exit "$test_status"
python3 - \
    "$EVIDENCE_DIR/relay.json" \
    "$EVIDENCE_DIR/direct.json" \
    "$EVIDENCE_DIR/report.json" \
    "$revision" \
    "$MAXIMUM_DIRECT_AVERAGE_MILLISECONDS" \
    "$MAXIMUM_DIRECT_P95_MILLISECONDS" \
    "$MAXIMUM_RELAY_AVERAGE_MILLISECONDS" \
    "$MAXIMUM_RELAY_P95_MILLISECONDS" \
    "$MAXIMUM_RELAY_P95_INCREMENT_MILLISECONDS" \
    "$MAXIMUM_AGENT_RSS_KIB" \
    "$MAXIMUM_AGENT_IDLE_CPU_PERCENT" \
    "$MAXIMUM_AGENT_FILE_DESCRIPTORS" \
    "$MAXIMUM_AGENT_THREADS" \
    "$EXPECTED_RTT_SAMPLES" <<'PY'
import json
import sys
from pathlib import Path

relay = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
direct = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
idle = json.loads((Path(sys.argv[1]).parent / "agent-idle.json").read_text(encoding="utf-8"))
direct_warmup = json.loads(
    (Path(sys.argv[1]).parent / "direct-warmup.json").read_text(encoding="utf-8")
)
envelope = {
    "maximum_direct_average_milliseconds": int(sys.argv[5]),
    "maximum_direct_p95_milliseconds": int(sys.argv[6]),
    "maximum_relay_average_milliseconds": int(sys.argv[7]),
    "maximum_relay_p95_milliseconds": int(sys.argv[8]),
    "maximum_relay_p95_increment_milliseconds": int(sys.argv[9]),
    "maximum_agent_rss_kib": int(sys.argv[10]),
    "maximum_agent_idle_cpu_percent_one_core": int(sys.argv[11]),
    "maximum_agent_file_descriptors": int(sys.argv[12]),
    "maximum_agent_threads": int(sys.argv[13]),
}
report = {
    "schema_version": 1,
    "git_revision": sys.argv[4],
    "samples_per_path": relay["count"],
    "relay": relay,
    "direct": direct,
    "direct_warmup": direct_warmup,
    "relay_average_increment_ms": relay["average_ms"] - direct["average_ms"],
    "relay_p95_increment_ms": relay["p95_ms"] - direct["p95_ms"],
    "agent_idle": idle,
    "safe_operating_envelope": envelope,
}
assert relay["count"] == direct["count"] == int(sys.argv[14])
assert direct_warmup["count"] == 10
assert direct["average_ms"] <= envelope["maximum_direct_average_milliseconds"]
assert direct["p95_ms"] <= envelope["maximum_direct_p95_milliseconds"]
assert relay["average_ms"] <= envelope["maximum_relay_average_milliseconds"]
assert relay["p95_ms"] <= envelope["maximum_relay_p95_milliseconds"]
assert report["relay_p95_increment_ms"] <= envelope[
    "maximum_relay_p95_increment_milliseconds"
]
assert idle["build_profile"] == "release"
assert idle["sample_duration_seconds"] >= 9.5
assert len(idle["agents"]) == 2
for agent in idle["agents"]:
    assert agent["rss_kib"] <= envelope["maximum_agent_rss_kib"]
    assert agent["idle_cpu_percent_one_core"] <= envelope[
        "maximum_agent_idle_cpu_percent_one_core"
    ]
    assert agent["fds"] <= envelope["maximum_agent_file_descriptors"]
    assert agent["threads"] <= envelope["maximum_agent_threads"]
Path(sys.argv[3]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY
secret_log=$(mktemp)
python3 scripts/check-secrets.py --root "$EVIDENCE_DIR" >"$secret_log" 2>&1
mv -- "$secret_log" "$EVIDENCE_DIR/secret-scan.log"
printf 'status=PASS\nrevision=%s\n' "$revision" >"$EVIDENCE_DIR/status.txt"
checksum_file=$(mktemp)
(
    cd "$EVIDENCE_DIR"
    sha256sum \
        agent-idle.json \
        command.txt \
        direct.json \
        direct-warmup.json \
        elapsed-seconds.txt \
        environment.txt \
        relay.json \
        report.json \
        revision.txt \
        secret-scan.log \
        status.txt \
        test-output.txt
) >"$checksum_file"
mv -- "$checksum_file" "$EVIDENCE_DIR/SHA256SUMS"
printf 'Agent Direct/Relay RTT baseline passed; evidence: %s\n' "$EVIDENCE_DIR"
