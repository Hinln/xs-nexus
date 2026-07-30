#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m31-acl.XXXXXX)
NETNS_A="xsm31aa-$$"
NETNS_B="xsm31ab-$$"
VETH_A=xsm31ava
VETH_B=xsm31avb
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
    rm -rf "$FIXTURE_ROOT"
    exit "$status"
}
trap cleanup EXIT INT TERM

wait_for_interface() {
    local namespace=$1
    local interface=$2
    for _ in $(seq 1 50); do
        if ip netns exec "$namespace" ip link show dev "$interface" >/dev/null 2>&1; then
            return
        fi
        sleep 0.1
    done
    printf 'timed out waiting for %s in %s\n' "$interface" "$namespace" >&2
    exit 1
}

assert_agents_alive() {
    kill -0 "$AGENT_A_PID"
    kill -0 "$AGENT_B_PID"
}

if [[ $(id -u) -ne 0 || ! -c /dev/net/tun ]]; then
    printf 'privileged ACL tests require root and /dev/net/tun\n' >&2
    exit 2
fi
for command in cargo ip python3; do
    command -v "$command" >/dev/null
done
if [[ -e /sys/class/net/xsa0 || -e /sys/class/net/xsb0 ]]; then
    printf 'refusing to run while host test TUN interfaces exist\n' >&2
    exit 2
fi

cd "$ROOT_DIR"
umask 077
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
XS_FIXTURE_ACL_MODE=acl_matrix \
    cargo run -q -p xs-agent --features privileged-network-tests \
    --example generate_pair_fixture -- \
    "$FIXTURE_ROOT" \
    10.231.0.1:42001 \
    10.231.0.2:42001 >"$FIXTURE_ROOT/manifest.json"

ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip link add "$VETH_A" type veth peer name "$VETH_B"
ip link set "$VETH_A" netns "$NETNS_A"
ip link set "$VETH_B" netns "$NETNS_B"
ip -n "$NETNS_A" link set lo up
ip -n "$NETNS_B" link set lo up
ip -n "$NETNS_A" addr add 10.231.0.1/30 dev "$VETH_A"
ip -n "$NETNS_B" addr add 10.231.0.2/30 dev "$VETH_B"
ip -n "$NETNS_A" link set "$VETH_A" up
ip -n "$NETNS_B" link set "$VETH_B" up

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
assert_agents_alive

ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 100.127.253.2 \
    --payload xs-m31-acl-icmp-a \
    --sequence 1
ip netns exec "$NETNS_B" "$PROBE" icmp \
    --destination 100.127.253.1 \
    --payload xs-m31-acl-icmp-b \
    --sequence 2

ip netns exec "$NETNS_B" "$PROBE" tcp-server \
    --bind 100.127.253.2 \
    --port 43111 \
    --request xs-m31-allowed-tcp-request \
    --response xs-m31-allowed-tcp-response \
    >"$FIXTURE_ROOT/tcp-allowed-server.log" 2>&1 &
tcp_allowed_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" tcp-client \
    --destination 100.127.253.2 \
    --port 43111 \
    --request xs-m31-allowed-tcp-request \
    --response xs-m31-allowed-tcp-response
wait "$tcp_allowed_pid"

ip netns exec "$NETNS_B" "$PROBE" tcp-server \
    --bind 100.127.253.2 \
    --port 43112 \
    --request xs-m31-blocked-tcp-request \
    --response xs-m31-blocked-tcp-response \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/tcp-blocked-server.log" 2>&1 &
tcp_blocked_pid=$!
ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$VETH_A" \
    --source 10.231.0.1 \
    --destination 10.231.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/tcp-blocked-capture.log" 2>&1 &
tcp_blocked_capture_pid=$!
sleep 0.2
if ip netns exec "$NETNS_A" "$PROBE" tcp-client \
    --destination 100.127.253.2 \
    --port 43112 \
    --request xs-m31-blocked-tcp-request \
    --response xs-m31-blocked-tcp-response \
    --timeout 1 \
    >"$FIXTURE_ROOT/tcp-blocked-client.log" 2>&1; then
    printf 'blocked TCP flow unexpectedly succeeded\n' >&2
    exit 1
fi
wait "$tcp_blocked_capture_pid"
if wait "$tcp_blocked_pid"; then
    printf 'blocked TCP server unexpectedly received a connection\n' >&2
    exit 1
fi

ip netns exec "$NETNS_B" "$PROBE" udp-server \
    --bind 100.127.253.2 \
    --port 43113 \
    --request xs-m31-allowed-udp-request \
    --response xs-m31-allowed-udp-response \
    >"$FIXTURE_ROOT/udp-allowed-server.log" 2>&1 &
udp_allowed_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" udp-client \
    --destination 100.127.253.2 \
    --port 43113 \
    --request xs-m31-allowed-udp-request \
    --response xs-m31-allowed-udp-response
wait "$udp_allowed_pid"

ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43114 \
    --expected xs-m31-blocked-udp \
    --count 0 \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/udp-blocked-server.log" 2>&1 &
udp_blocked_pid=$!
ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$VETH_A" \
    --source 10.231.0.1 \
    --destination 10.231.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/udp-blocked-capture.log" 2>&1 &
udp_blocked_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.1 \
    --destination 100.127.253.2 \
    --source-port 53114 \
    --destination-port 43114 \
    --payload xs-m31-blocked-udp \
    --packet-id 114
wait "$udp_blocked_capture_pid"
wait "$udp_blocked_pid"

ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43115 \
    --expected xs-m31-receiver-deny \
    --count 0 \
    --timeout 2 \
    >"$FIXTURE_ROOT/receiver-deny-server.log" 2>&1 &
receiver_deny_pid=$!
ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
    --interface "$VETH_A" \
    --source 10.231.0.1 \
    --destination 10.231.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --count 1 \
    --timeout 2 \
    --forbid-text xs-m31-receiver-deny \
    >"$FIXTURE_ROOT/receiver-deny-capture.log" 2>&1 &
receiver_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.1 \
    --destination 100.127.253.2 \
    --source-port 53115 \
    --destination-port 43115 \
    --payload xs-m31-receiver-deny \
    --packet-id 115
wait "$receiver_capture_pid"
wait "$receiver_deny_pid"

assert_agents_alive
if [[ -s $FIXTURE_ROOT/agent-a.log || -s $FIXTURE_ROOT/agent-b.log ]]; then
    cat "$FIXTURE_ROOT/agent-a.log" "$FIXTURE_ROOT/agent-b.log" >&2
    exit 1
fi
printf 'Agent ACL namespace tests passed\n'
