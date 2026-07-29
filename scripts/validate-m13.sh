#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m1.3-$STAMP"
LOG_FILE="$QA_DIR/validate-m13.log"
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

printf 'M1.3 validation started at %s\n' "$(date -u --iso-8601=seconds)"
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)

env GIT_PAGER=cat git diff --check
make fmt-check
make lint
cargo clippy -p xs-agent --all-targets --features privileged-network-tests -- -D warnings
make build
make test
cargo test -p xs-protocol --all-targets
cargo test -p xs-agent --lib --bins --examples --features privileged-network-tests
make test-network
make test-agent-data-plane
make security-check
python3 -m py_compile scripts/xsp-network-probe.py
shellcheck scripts/*.sh
npm audit --audit-level=high

test ! -e /sys/class/net/xsa0
test ! -e /sys/class/net/xsb0
if ip netns list | awk '{print $1}' | grep -Eq '^xsm13[ab]-'; then
    ip netns list >&2
    exit 1
fi
if nft list table inet xsm13 >/dev/null 2>&1; then
    nft list table inet xsm13 >&2
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

printf 'M1.3 validation passed at %s\n' "$(date -u --iso-8601=seconds)"
