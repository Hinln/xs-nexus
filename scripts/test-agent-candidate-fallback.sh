#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m21-fallback.XXXXXX)
SUFFIX=$$
NETNS_A="xsm2fa-$SUFFIX"
NETNS_B="xsm2fb-$SUFFIX"
VETH_A="xm2fva$SUFFIX"
VETH_B="xm2fvb$SUFFIX"
AGENT_A_PID=
AGENT_B_PID=

report_failure() {
    local status=$?
    for log in "$FIXTURE_ROOT"/agent-a.log "$FIXTURE_ROOT"/agent-b.log; do
        if [[ -f $log ]]; then
            printf '%s\n' "=== $(basename "$log") ===" >&2
            tail -n 80 "$log" >&2
        fi
    done
    return "$status"
}

cleanup() {
    local status=$?
    trap - ERR
    set +e
    for namespace in "$NETNS_A" "$NETNS_B"; do
        if ip netns list | awk '{print $1}' | grep -Fxq "$namespace"; then
            ip netns exec "$namespace" nft delete table inet xsm2f >/dev/null 2>&1
        fi
    done
    for pid in "$AGENT_A_PID" "$AGENT_B_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
            wait "$pid" >/dev/null 2>&1
        fi
    done
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    rm -rf "$FIXTURE_ROOT"
    exit "$status"
}
trap report_failure ERR
trap cleanup EXIT INT TERM

require_command() {
    command -v "$1" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$1" >&2
        exit 2
    }
}

wait_for_interface() {
    local namespace=$1
    local interface=$2
    for _ in $(seq 1 80); do
        if ip netns exec "$namespace" ip link show dev "$interface" >/dev/null 2>&1; then
            return
        fi
        sleep 0.1
    done
    printf 'timed out waiting for %s in %s\n' "$interface" "$namespace" >&2
    exit 1
}

if [[ $(id -u) -ne 0 ]] || [[ ! -c /dev/net/tun ]]; then
    printf 'candidate fallback test requires root and /dev/net/tun\n' >&2
    exit 2
fi
for command in cargo ip nft python3; do
    require_command "$command"
done

cd "$ROOT_DIR"
umask 077
python3 -m py_compile "$PROBE"
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli
cargo run -q -p xs-agent --features privileged-network-tests \
    --example generate_pair_fixture -- \
    "$FIXTURE_ROOT" \
    10.204.0.1:42001 \
    10.204.0.2:42001 \
    10.204.0.5:42001 \
    10.204.0.6:42001 >"$FIXTURE_ROOT/manifest.json"

ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip link add "$VETH_A" type veth peer name "$VETH_B"
ip link set "$VETH_A" netns "$NETNS_A"
ip link set "$VETH_B" netns "$NETNS_B"
ip -n "$NETNS_A" link set lo up
ip -n "$NETNS_B" link set lo up
ip -n "$NETNS_A" addr add 10.204.0.1/29 dev "$VETH_A"
ip -n "$NETNS_B" addr add 10.204.0.2/29 dev "$VETH_B"
ip -n "$NETNS_A" link set "$VETH_A" up
ip -n "$NETNS_B" link set "$VETH_B" up
ip netns exec "$NETNS_A" nft add table inet xsm2f
ip netns exec "$NETNS_A" nft add chain inet xsm2f output \
    '{ type filter hook output priority 0; policy accept; }'
ip netns exec "$NETNS_A" nft add rule inet xsm2f output \
    ip daddr 10.204.0.6 udp dport 42001 counter

ip netns exec "$NETNS_A" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-a/agent.json" \
    >"$FIXTURE_ROOT/agent-a.log" 2>&1 &
AGENT_A_PID=$!
ip netns exec "$NETNS_B" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-b/agent.json" \
    >"$FIXTURE_ROOT/agent-b.log" 2>&1 &
AGENT_B_PID=$!
wait_for_interface "$NETNS_A" xsa0
wait_for_interface "$NETNS_B" xsb0

ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 100.127.253.2 \
    --payload xs-m21-candidate-fallback \
    --sequence 1 \
    --timeout 6
ip netns exec "$NETNS_B" "$PROBE" icmp \
    --destination 100.127.253.1 \
    --payload xs-m21-candidate-reverse \
    --sequence 2 \
    --timeout 3

ip netns exec "$NETNS_A" nft list table inet xsm2f |
    grep -Eq 'packets [1-9][0-9]* bytes [1-9][0-9]*'

peer_json=$("$CLI" peers --socket "$FIXTURE_ROOT/node-a/run/agent.sock" --json)
python3 - "$peer_json" <<'PY'
import json
import sys

response = json.loads(sys.argv[1])
peer = response["peers"][0]
if peer["active_endpoint"] != "10.204.0.2:42001":
    raise SystemExit(f"wrong active endpoint: {peer['active_endpoint']}")
if peer["path_reason"] != "handshake_fallback":
    raise SystemExit(f"fallback reason not recorded: {peer['path_reason']}")
if not peer["session_established"] or len(peer["candidates"]) != 2:
    raise SystemExit(f"candidate fallback status invalid: {peer}")
if peer["candidates"][0]["endpoint"] != "10.204.0.6:42001":
    raise SystemExit(f"first candidate was not the unreachable endpoint: {peer}")
PY

printf 'Agent multi-candidate handshake fallback test passed\n'
