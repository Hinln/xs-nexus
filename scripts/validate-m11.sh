#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m1.1-$STAMP"
LOG_FILE="$QA_DIR/validate-m11.log"
SUMMARY_FILE="$QA_DIR/summary.txt"
CONTROLLER_LOG="$QA_DIR/controller-smoke.log"
CONTROLLER_PID=""

mkdir -p "$QA_DIR"

finish() {
    local status=$?
    if [[ -n "$CONTROLLER_PID" ]] && kill -0 "$CONTROLLER_PID" 2>/dev/null; then
        kill -TERM "$CONTROLLER_PID" 2>/dev/null || true
        wait "$CONTROLLER_PID" 2>/dev/null || true
    fi
    printf 'validation_status=%s\n' "$status" | tee "$SUMMARY_FILE"
    printf 'evidence=%s\n' "$QA_DIR" | tee -a "$SUMMARY_FILE"
}
trap finish EXIT

cd "$ROOT_DIR"
exec > >(tee "$LOG_FILE") 2>&1

printf 'M1.1 validation started at %s\n' "$(date -u --iso-8601=seconds)"
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')

env GIT_PAGER=cat git diff --check
make fmt-check
make lint
make build
make test
make test-network
make security-check
shellcheck scripts/*.sh
npm audit --audit-level=high

docker compose --profile baseline -f deploy/docker/compose.yaml config \
    | grep -A4 '^networks:' \
    | grep -F 'external: true'
network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
printf '%s\n' "$network_definition" | grep -Fx '1panel-network 172.18.0.0/16 '

mapfile -d '' -t controller_settings < <(python3 - <<'PY'
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

values = {}
for raw_line in Path("/etc/xs-nexus/controller.env").read_text().splitlines():
    line = raw_line.strip()
    if line and not line.startswith("#") and "=" in line:
        name, value = line.split("=", 1)
        values[name] = value

parsed = urlsplit(values["DATABASE_URL"])
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
values["DATABASE_URL"] = urlunsplit(
    (parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)
)
values["DATABASE_SCHEMA"] = values.get("XS_TEST_DATABASE_SCHEMA", "xs_nexus_test")
values["CONTROLLER_LISTEN"] = "127.0.0.1:18080"

for name in (
    "DATABASE_URL",
    "DATABASE_SCHEMA",
    "CONTROLLER_LISTEN",
    "ADMIN_API_TOKEN",
    "CREDENTIAL_SIGNING_KEY_PATH",
    "CONFIG_SIGNING_KEY_PATH",
    "NODE_CREDENTIAL_TTL_SECONDS",
):
    value = values.get(name, "")
    print(name, end="\0")
    print(value, end="\0")
PY
)

for ((index = 0; index < ${#controller_settings[@]}; index += 2)); do
    printf -v "${controller_settings[index]}" '%s' "${controller_settings[index + 1]}"
    export "${controller_settings[index]}"
done

target/debug/xs-controller >"$CONTROLLER_LOG" 2>&1 &
CONTROLLER_PID=$!
for _ in $(seq 1 50); do
    if curl --fail --silent http://127.0.0.1:18080/health/ready >/dev/null; then
        break
    fi
    sleep 0.1
done
curl --fail --silent http://127.0.0.1:18080/health/live | grep -F '"status":"ok"'
curl --fail --silent http://127.0.0.1:18080/health/ready | grep -F '"database":"ok"'
kill -TERM "$CONTROLLER_PID"
wait "$CONTROLLER_PID"
CONTROLLER_PID=""

if ss -ltn 'sport = :18080' | grep -q LISTEN; then
    printf 'controller smoke listener remained active\n' >&2
    exit 1
fi

python3 - "$CONTROLLER_LOG" <<'PY'
import os
import sys
from pathlib import Path

data = Path(sys.argv[1]).read_bytes()
for name in ("DATABASE_URL", "ADMIN_API_TOKEN"):
    value = os.environ.get(name, "").encode()
    if len(value) >= 8 and value in data:
        raise SystemExit(f"controller smoke log leaked {name}")
PY

docker_after=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
network_members_after=$(docker network inspect 1panel-network --format '{{json .Containers}}')
[[ "$docker_before" == "$docker_after" ]]
[[ "$network_members_before" == "$network_members_after" ]]

if systemctl --failed --no-legend | grep -q '[^[:space:]]'; then
    systemctl --failed --no-pager >&2
    exit 1
fi

printf 'M1.1 validation passed at %s\n' "$(date -u --iso-8601=seconds)"
