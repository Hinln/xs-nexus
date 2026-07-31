#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m2.3-$STAMP"
LOG_FILE="$QA_DIR/validate-m23.log"
SUMMARY_FILE="$QA_DIR/summary.txt"
FIREWALL_BEFORE="$QA_DIR/nftables-before.json"
FIREWALL_AFTER="$QA_DIR/nftables-after.json"

mkdir -p "$QA_DIR"

finish() {
    local status=$?
    printf 'validation_status=%s\n' "$status" | tee "$SUMMARY_FILE"
    printf 'evidence=%s\n' "$QA_DIR" | tee -a "$SUMMARY_FILE"
}
trap finish EXIT

snapshot_firewall() {
    nft -j list ruleset | python3 -c '
import json
import sys


def normalize(value):
    if isinstance(value, dict):
        normalized = {key: normalize(item) for key, item in value.items()}
        counter = normalized.get("counter")
        if isinstance(counter, dict):
            counter.pop("packets", None)
            counter.pop("bytes", None)
        return normalized
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


json.dump(normalize(json.load(sys.stdin)), sys.stdout, sort_keys=True, separators=(",", ":"))
'
}

cd "$ROOT_DIR"
exec > >(tee "$LOG_FILE") 2>&1

printf 'M2.3 validation started at %s\n' "$(date -u --iso-8601=seconds)"
printf 'git_head=%s\n' "$(git rev-parse HEAD)"
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_before=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_BEFORE"

env GIT_PAGER=cat git diff --check
make fmt-check
make lint
cargo clippy -p xs-agent --all-targets --features privileged-network-tests -- -D warnings
make build
make test
cargo test -p xs-protocol --all-targets
cargo test -p xs-relay --all-targets
cargo test -p xs-agent --lib --bins --examples --features privileged-network-tests
make test-network
make test-agent-data-plane
make test-agent-candidates
make test-agent-nat
make test-agent-relay
make security-check
python3 -m py_compile scripts/xsp-network-probe.py
shellcheck scripts/*.sh
npm audit --audit-level=high

env \
    XS_RELEASE_REVISION="$(git rev-parse HEAD)" \
    XS_CONTROLLER_IMAGE=validation/controller \
    XS_MIGRATION_IMAGE=validation/controller \
    XS_RELAY_IMAGE=validation/relay \
    XS_CONSOLE_IMAGE=validation/console \
    XS_DB_TOOLS_IMAGE=validation/db-tools \
    XS_CONTROLLER_SECRETS_DIR=/nonexistent/controller \
    XS_RELAY_SECRETS_DIR=/nonexistent/relay \
    XS_BACKUP_DIR=/nonexistent/backup \
    XS_DATABASE_SCHEMA=xs_nexus_validation \
    XS_DISCOVERY_PUBLIC_ENDPOINT=127.0.0.1:42000 \
    XS_RELAY_ID_BASE64=AAAAAAAAAAAAAAAAAAAAAQ \
    docker compose --profile baseline -f deploy/docker/compose.yaml config \
    | grep -A4 '^networks:' \
    | grep -F 'external: true'
network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
printf '%s\n' "$network_definition" | grep -Fx '1panel-network 172.18.0.0/16 '

if ip netns list | awk '{print $1}' |
    grep -Eq '^(xsm13|xsm2f|xsm21|xsm22|xsm23|x22)'; then
    ip netns list >&2
    exit 1
fi
if ip -o link show | awk -F': ' '{print $2}' | cut -d@ -f1 |
    grep -Eq '^(xm13|xmm2f|xm21|xm22|xm23|m[1-6](a|b|r|w|n))'; then
    ip -o link show >&2
    exit 1
fi
for interface in xsa0 xsb0; do
    test ! -e "/sys/class/net/$interface"
done

docker_after=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_after=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_after=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_after=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_AFTER"
[[ "$docker_before" == "$docker_after" ]]
[[ "$docker_networks_before" == "$docker_networks_after" ]]
[[ "$network_members_before" == "$network_members_after" ]]
[[ "$default_routes_before" == "$default_routes_after" ]]
cmp -s "$FIREWALL_BEFORE" "$FIREWALL_AFTER"

if systemctl --failed --no-legend | grep -q '[^[:space:]]'; then
    systemctl --failed --no-pager >&2
    exit 1
fi

printf 'M2.3 validation passed at %s\n' "$(date -u --iso-8601=seconds)"
