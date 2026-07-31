#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m6.1-agent-session-$STAMP"
LOG_FILE="$QA_DIR/validate-m61-agent-session.log"
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

printf 'M6.1 Agent session validation started at %s\n' "$(date -u --iso-8601=seconds)"
printf 'git_head=%s\n' "$(git rev-parse HEAD)"
docker_before=$(docker ps --format '{{.ID}} {{.Names}} {{.Image}} {{.Ports}}' | sort)
docker_networks_before=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
network_members_before=$(docker network inspect 1panel-network --format '{{json .Containers}}')
default_routes_before=$(ip -json route show default)
snapshot_firewall >"$FIREWALL_BEFORE"

env GIT_PAGER=cat git diff --check
make fmt-check
make lint
make test-unit
make test-agent-control
make test-windows-xsnet-abi
make test-windows-xsnet-source
make test-windows-xsnet-installer
make test-windows-xsnet-vm-scripts
make test-windows-xsnet-compatibility
make test-windows-xsnet-transport
make test-windows-agent-ipc
make test-windows-agent-storage
make test-windows-agent-routing
make test-windows-agent-service
make test-independent-implementation
make test-source-sbom
make security-check
shellcheck scripts/*.sh installers/linux/*.sh deploy/docker/*.sh
npm audit --audit-level=high

network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{.Driver}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
printf '%s\n' "$network_definition" | grep -Fx '1panel-network bridge 172.18.0.0/16 '

if ip netns list | awk '{print $1}' |
    grep -Eq '^(xsm13|xsm2f|xsm21|xsm22|xsm23|xsm31|xsm32|x22)'; then
    ip netns list >&2
    exit 1
fi
if ip -o link show | awk -F': ' '{print $2}' | cut -d@ -f1 |
    grep -Eq '^(xm13|xmm2f|xm21|xm22|xm23|xsm31|m[1-6](a|b|r|w|n))'; then
    ip -o link show >&2
    exit 1
fi
for interface in xsa0 xsb0 xstest0 xssvc0; do
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

printf 'M6.1 Agent session validation passed at %s\n' "$(date -u --iso-8601=seconds)"
