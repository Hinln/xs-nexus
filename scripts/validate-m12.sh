#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m1.2-$STAMP"
LOG_FILE="$QA_DIR/validate-m12.log"
SUMMARY_FILE="$QA_DIR/summary.txt"

mkdir -p "$QA_DIR"

finish() {
    local status=$?
    printf 'validation_status=%s\n' "$status" | tee "$SUMMARY_FILE"
    printf 'evidence=%s\n' "$QA_DIR" | tee -a "$SUMMARY_FILE"
}
trap finish EXIT

cd "$ROOT_DIR"
exec > >(tee "$LOG_FILE") 2>&1

printf 'M1.2 validation started at %s\n' "$(date -u --iso-8601=seconds)"
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)

env GIT_PAGER=cat git diff --check
make fmt-check
make lint
cargo clippy -p xs-agent --all-targets --features privileged-network-tests -- -D warnings
make build
make test
cargo test -p xs-agent --test ipc -- --test-threads=1
cargo test -p xs-cli --all-targets -- --test-threads=1
make test-network
make security-check
shellcheck scripts/*.sh
npm audit --audit-level=high

grep -F 'User=xs-nexus' deploy/systemd/xs-agent.service
grep -F 'CapabilityBoundingSet=CAP_NET_ADMIN' deploy/systemd/xs-agent.service
grep -F 'DeviceAllow=/dev/net/tun rw' deploy/systemd/xs-agent.service
grep -F 'NoNewPrivileges=yes' deploy/systemd/xs-agent.service
grep -F 'ProtectSystem=strict' deploy/systemd/xs-agent.service
test ! -e /sys/class/net/xstest0
test ! -e /sys/class/net/xssvc0
if systemctl list-units --all --no-legend 'xs-agent-m12-*' | grep -q '[^[:space:]]'; then
    systemctl list-units --all 'xs-agent-m12-*' --no-pager >&2
    exit 1
fi

docker_after=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
network_members_after=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_after=$(ip -json route show default)
[[ "$docker_before" == "$docker_after" ]]
[[ "$network_members_before" == "$network_members_after" ]]
[[ "$default_routes_before" == "$default_routes_after" ]]

if systemctl --failed --no-legend | grep -q '[^[:space:]]'; then
    systemctl --failed --no-pager >&2
    exit 1
fi

printf 'M1.2 validation passed at %s\n' "$(date -u --iso-8601=seconds)"
