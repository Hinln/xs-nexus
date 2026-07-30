#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m22-proactive.XXXXXX)
SUFFIX=$(printf '%04x' "$(( $$ % 65536 ))")
NETNS_A="xsm22pa-$SUFFIX"
NETNS_B="xsm22pb-$SUFFIX"
VETH_A="xm22pa$SUFFIX"
VETH_B="xm22pb$SUFFIX"
AGENT_A_PID=
AGENT_B_PID=

cleanup() {
    local status=$?
    set +e
    for pid in "$AGENT_A_PID" "$AGENT_B_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
            wait "$pid" >/dev/null 2>&1
        fi
    done
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    if (( status != 0 )); then
        for log in agent-a.log agent-b.log; do
            if [[ -s $FIXTURE_ROOT/$log ]]; then
                printf '\n--- %s ---\n' "$log" >&2
                cat "$FIXTURE_ROOT/$log" >&2
            fi
        done
    fi
    rm -rf "$FIXTURE_ROOT"
    exit "$status"
}
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
    for _ in $(seq 1 100); do
        if ip netns exec "$namespace" ip link show dev "$interface" >/dev/null 2>&1; then
            return
        fi
        sleep 0.1
    done
    printf 'timed out waiting for %s in %s\n' "$interface" "$namespace" >&2
    exit 1
}

wait_for_session() {
    local socket=$1
    for _ in $(seq 1 120); do
        local response
        response=$($CLI peers --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]] && python3 - "$response" <<'PY'
import json
import sys

response = json.loads(sys.argv[1])
raise SystemExit(not response["peers"][0]["session_established"])
PY
        then
            return
        fi
        sleep 0.1
    done
    printf 'proactive authenticated session did not become ready: %s\n' "$socket" >&2
    $CLI peers --socket "$socket" --json >&2 || true
    exit 1
}

outbound_packet_count() {
    local namespace=$1
    ip netns exec "$namespace" nft list table inet xsm22observe |
        awk '
            /counter packets/ {
                for (field = 1; field <= NF; field++) {
                    if ($field == "packets") {
                        print $(field + 1)
                        exit
                    }
                }
            }
        '
}

if [[ $(id -u) -ne 0 ]] || [[ ! -c /dev/net/tun ]]; then
    printf 'proactive punch test requires root and /dev/net/tun\n' >&2
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
    10.205.0.1:43001 \
    10.205.0.2:43001 >"$FIXTURE_ROOT/manifest.json"

ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip link add "$VETH_A" type veth peer name "$VETH_B"
ip link set "$VETH_A" netns "$NETNS_A"
ip link set "$VETH_B" netns "$NETNS_B"
ip -n "$NETNS_A" link set lo up
ip -n "$NETNS_B" link set lo up
ip -n "$NETNS_A" addr add 10.205.0.1/30 dev "$VETH_A"
ip -n "$NETNS_B" addr add 10.205.0.2/30 dev "$VETH_B"
ip -n "$NETNS_A" link set "$VETH_A" up
ip -n "$NETNS_B" link set "$VETH_B" up
ip netns exec "$NETNS_A" nft add table inet xsm22observe
ip netns exec "$NETNS_A" nft add chain inet xsm22observe output \
    '{ type filter hook output priority 0; policy accept; }'
ip netns exec "$NETNS_A" nft add rule inet xsm22observe output \
    ip daddr 10.205.0.2 udp dport 43001 counter

ip netns exec "$NETNS_A" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-a/agent.json" >"$FIXTURE_ROOT/agent-a.log" 2>&1 &
AGENT_A_PID=$!
ip netns exec "$NETNS_B" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-b/agent.json" >"$FIXTURE_ROOT/agent-b.log" 2>&1 &
AGENT_B_PID=$!
wait_for_interface "$NETNS_A" xsa0
wait_for_interface "$NETNS_B" xsb0

wait_for_session "$FIXTURE_ROOT/node-a/run/agent.sock"
wait_for_session "$FIXTURE_ROOT/node-b/run/agent.sock"
packets_before_keepalive=$(outbound_packet_count "$NETNS_A")
sleep 1.5
packets_after_keepalive=$(outbound_packet_count "$NETNS_A")
if [[ ! $packets_before_keepalive =~ ^[0-9]+$ ]] ||
    [[ ! $packets_after_keepalive =~ ^[0-9]+$ ]] ||
    (( packets_after_keepalive <= packets_before_keepalive )); then
    printf 'authenticated keepalive did not maintain the UDP path: before=%s after=%s\n' \
        "$packets_before_keepalive" "$packets_after_keepalive" >&2
    exit 1
fi

ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 100.127.253.2 \
    --payload xs-m22-proactive-direct \
    --sequence 1 \
    --timeout 3
ip netns exec "$NETNS_B" "$PROBE" icmp \
    --destination 100.127.253.1 \
    --payload xs-m22-proactive-reverse \
    --sequence 2 \
    --timeout 3

printf 'Agent proactive authenticated direct session test passed\n'
