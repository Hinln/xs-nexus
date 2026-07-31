#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/agent-rtt-$(date -u +%Y%m%dT%H%M%SZ)"}
mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
cd "$ROOT_DIR"
git rev-parse HEAD >"$EVIDENCE_DIR/revision.txt"
XS_AGENT_TEST_PROFILE=release RTT_EVIDENCE_DIR="$EVIDENCE_DIR" ./scripts/test-agent-relay.sh \
    >"$EVIDENCE_DIR/test-output.txt" 2>&1
python3 - "$EVIDENCE_DIR/relay.json" "$EVIDENCE_DIR/direct.json" "$EVIDENCE_DIR/report.json" <<'PY'
import json
import sys
from pathlib import Path

relay = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
direct = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
idle = json.loads((Path(sys.argv[1]).parent / "agent-idle.json").read_text(encoding="utf-8"))
report = {
    "samples_per_path": relay["count"],
    "relay": relay,
    "direct": direct,
    "relay_average_increment_ms": relay["average_ms"] - direct["average_ms"],
    "relay_p95_increment_ms": relay["p95_ms"] - direct["p95_ms"],
    "agent_idle": idle,
}
assert relay["count"] == direct["count"] == 30
Path(sys.argv[3]).write_text(
    json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
PY
printf 'Agent Direct/Relay RTT baseline passed; evidence: %s\n' "$EVIDENCE_DIR"
