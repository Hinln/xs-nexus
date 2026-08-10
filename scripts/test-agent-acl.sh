#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-gate09-acl.XXXXXX)
SUFFIX=$(printf '%x' "$$")
SUFFIX=${SUFFIX: -4}
NETNS_A="xsg09a-$SUFFIX"
NETNS_B="xsg09b-$SUFFIX"
NETNS_C="xsg09c-$SUFFIX"
BRIDGE="xsg9br$SUFFIX"
HOST_A="xsg9ha$SUFFIX"
HOST_B="xsg9hb$SUFFIX"
HOST_C="xsg9hc$SUFFIX"
LINK_A="xsg9na$SUFFIX"
LINK_B="xsg9nb$SUFFIX"
LINK_C="xsg9nc$SUFFIX"
UNDERLAY_A=10.233.0.1
UNDERLAY_B=10.233.0.2
UNDERLAY_C=10.233.0.3
VIRTUAL_A=100.127.253.1
VIRTUAL_B=100.127.253.2
VIRTUAL_C=100.127.253.3
AGENT_A_PID=
AGENT_B_PID=
AGENT_C_PID=
CAPTURE_PID=

report_failure() {
    local status=$?
    local log
    for log in "$FIXTURE_ROOT"/agent-a.log "$FIXTURE_ROOT"/agent-b.log \
        "$FIXTURE_ROOT"/agent-c.log; do
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
    for pid in "$CAPTURE_PID" "$AGENT_A_PID" "$AGENT_B_PID" "$AGENT_C_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
        fi
    done
    for pid in "$CAPTURE_PID" "$AGENT_A_PID" "$AGENT_B_PID" "$AGENT_C_PID"; do
        if [[ -n $pid ]]; then
            wait "$pid" >/dev/null 2>&1
        fi
    done
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    ip netns del "$NETNS_C" >/dev/null 2>&1
    ip link del "$BRIDGE" >/dev/null 2>&1
    rm -rf "$FIXTURE_ROOT"
    exit "$status"
}
trap report_failure ERR
trap cleanup EXIT INT TERM

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

assert_agents_alive() {
    kill -0 "$AGENT_A_PID"
    kill -0 "$AGENT_B_PID"
    kill -0 "$AGENT_C_PID"
}

assert_offline_configuration() {
    local socket=$1
    local response
    response=$("$CLI" status --socket "$socket" --json)
    python3 - "$response" <<'PY'
import json
import sys

status = json.loads(sys.argv[1])["status"]
assert status["network_active"] is True
assert status["controller_connected"] is False
assert status["configuration_version"] == 1
PY
}

start_no_xsp_capture() {
    local namespace=$1
    local interface=$2
    local underlay_source=$3
    local underlay_destination=$4
    local label=$5
    ip netns exec "$namespace" "$PROBE" assert-no-xsp \
        --interface "$interface" \
        --source "$underlay_source" \
        --destination "$underlay_destination" \
        --source-port 42001 \
        --destination-port 42001 \
        --packet-type data \
        --timeout 1.5 >"$FIXTURE_ROOT/$label-capture.log" 2>&1 &
    CAPTURE_PID=$!
    sleep 0.2
}

finish_no_xsp_capture() {
    wait "$CAPTURE_PID"
    CAPTURE_PID=
}

assert_icmp_denied() {
    local source_namespace=$1
    local source_interface=$2
    local underlay_source=$3
    local underlay_destination=$4
    local virtual_destination=$5
    local sequence=$6
    local label=$7
    start_no_xsp_capture "$source_namespace" "$source_interface" \
        "$underlay_source" "$underlay_destination" "$label"
    if ip netns exec "$source_namespace" "$PROBE" icmp \
        --destination "$virtual_destination" \
        --payload "$label" \
        --sequence "$sequence" \
        --timeout 1 >"$FIXTURE_ROOT/$label-client.log" 2>&1; then
        printf 'denied ICMP flow unexpectedly succeeded: %s\n' "$label" >&2
        exit 1
    fi
    finish_no_xsp_capture
}

assert_tcp_denied() {
    local source_namespace=$1
    local destination_namespace=$2
    local source_interface=$3
    local underlay_source=$4
    local underlay_destination=$5
    local virtual_destination=$6
    local port=$7
    local label=$8
    ip netns exec "$destination_namespace" "$PROBE" tcp-server \
        --bind "$virtual_destination" \
        --port "$port" \
        --request "$label-request" \
        --response "$label-response" \
        --timeout 1.5 >"$FIXTURE_ROOT/$label-server.log" 2>&1 &
    local server_pid=$!
    start_no_xsp_capture "$source_namespace" "$source_interface" \
        "$underlay_source" "$underlay_destination" "$label"
    if ip netns exec "$source_namespace" "$PROBE" tcp-client \
        --destination "$virtual_destination" \
        --port "$port" \
        --request "$label-request" \
        --response "$label-response" \
        --timeout 1 >"$FIXTURE_ROOT/$label-client.log" 2>&1; then
        printf 'denied TCP flow unexpectedly succeeded: %s\n' "$label" >&2
        exit 1
    fi
    finish_no_xsp_capture
    if wait "$server_pid"; then
        printf 'denied TCP server unexpectedly received a connection: %s\n' "$label" >&2
        exit 1
    fi
}

assert_udp_denied() {
    local source_namespace=$1
    local destination_namespace=$2
    local source_interface=$3
    local underlay_source=$4
    local underlay_destination=$5
    local virtual_source=$6
    local virtual_destination=$7
    local port=$8
    local label=$9
    ip netns exec "$destination_namespace" "$PROBE" collect-udp \
        --bind "$virtual_destination" \
        --port "$port" \
        --expected "$label" \
        --count 0 \
        --timeout 1.5 >"$FIXTURE_ROOT/$label-server.log" 2>&1 &
    local server_pid=$!
    start_no_xsp_capture "$source_namespace" "$source_interface" \
        "$underlay_source" "$underlay_destination" "$label"
    ip netns exec "$source_namespace" "$PROBE" send-virtual \
        --source "$virtual_source" \
        --destination "$virtual_destination" \
        --source-port 53300 \
        --destination-port "$port" \
        --payload "$label" \
        --packet-id "$port" >"$FIXTURE_ROOT/$label-client.log"
    finish_no_xsp_capture
    wait "$server_pid"
}

assert_receiver_tcp_denied() {
    local label=receiver-tcp-denied
    ip netns exec "$NETNS_B" "$PROBE" tcp-server \
        --bind "$VIRTUAL_B" \
        --port 43315 \
        --request "$label-request" \
        --response "$label-response" \
        --timeout 1.5 >"$FIXTURE_ROOT/$label-server.log" 2>&1 &
    local server_pid=$!
    ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
        --interface "$LINK_A" \
        --source "$UNDERLAY_A" \
        --destination "$UNDERLAY_B" \
        --source-port 42001 \
        --destination-port 42001 \
        --packet-type data \
        --count 1 \
        --timeout 2 >"$FIXTURE_ROOT/$label-capture.log" 2>&1 &
    CAPTURE_PID=$!
    sleep 0.2
    if ip netns exec "$NETNS_A" "$PROBE" tcp-client \
        --destination "$VIRTUAL_B" \
        --port 43315 \
        --request "$label-request" \
        --response "$label-response" \
        --timeout 1 >"$FIXTURE_ROOT/$label-client.log" 2>&1; then
        printf 'receiver-denied TCP flow unexpectedly succeeded\n' >&2
        exit 1
    fi
    wait "$CAPTURE_PID"
    CAPTURE_PID=
    if wait "$server_pid"; then
        printf 'receiver-denied TCP server unexpectedly received a connection\n' >&2
        exit 1
    fi
}

assert_receiver_udp_denied() {
    local label=receiver-udp-denied
    ip netns exec "$NETNS_B" "$PROBE" collect-udp \
        --bind "$VIRTUAL_B" \
        --port 43316 \
        --expected "$label" \
        --count 0 \
        --timeout 2 >"$FIXTURE_ROOT/$label-server.log" 2>&1 &
    local server_pid=$!
    ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
        --interface "$LINK_A" \
        --source "$UNDERLAY_A" \
        --destination "$UNDERLAY_B" \
        --source-port 42001 \
        --destination-port 42001 \
        --packet-type data \
        --count 1 \
        --timeout 2 \
        --forbid-text "$label" >"$FIXTURE_ROOT/$label-capture.log" 2>&1 &
    CAPTURE_PID=$!
    sleep 0.2
    ip netns exec "$NETNS_A" "$PROBE" send-virtual \
        --source "$VIRTUAL_A" \
        --destination "$VIRTUAL_B" \
        --source-port 53316 \
        --destination-port 43316 \
        --payload "$label" \
        --packet-id 316 >"$FIXTURE_ROOT/$label-client.log"
    wait "$CAPTURE_PID"
    CAPTURE_PID=
    wait "$server_pid"
}

if [[ $(id -u) -ne 0 || ! -c /dev/net/tun ]]; then
    printf 'privileged ACL tests require root and /dev/net/tun\n' >&2
    exit 2
fi
for command in cargo ip python3; do
    command -v "$command" >/dev/null
done
for interface in xsa0 xsb0 xsc0 "$BRIDGE" "$HOST_A" "$HOST_B" "$HOST_C"; do
    if [[ -e /sys/class/net/$interface ]]; then
        printf 'refusing to run while host test interface exists: %s\n' "$interface" >&2
        exit 2
    fi
done

cd "$ROOT_DIR"
umask 077
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli
XS_FIXTURE_ACL_MODE=acl_gate \
XS_FIXTURE_NODE_C_ENDPOINT="$UNDERLAY_C:42001" \
    cargo run -q -p xs-agent --features privileged-network-tests \
    --example generate_pair_fixture -- \
    "$FIXTURE_ROOT" \
    "$UNDERLAY_A:42001" \
    "$UNDERLAY_B:42001" >"$FIXTURE_ROOT/manifest.json"

ip link add "$BRIDGE" type bridge
ip link set "$BRIDGE" up
for namespace in "$NETNS_A" "$NETNS_B" "$NETNS_C"; do
    ip netns add "$namespace"
    ip -n "$namespace" link set lo up
done
ip link add "$HOST_A" type veth peer name "$LINK_A"
ip link add "$HOST_B" type veth peer name "$LINK_B"
ip link add "$HOST_C" type veth peer name "$LINK_C"
ip link set "$LINK_A" netns "$NETNS_A"
ip link set "$LINK_B" netns "$NETNS_B"
ip link set "$LINK_C" netns "$NETNS_C"
for host_interface in "$HOST_A" "$HOST_B" "$HOST_C"; do
    ip link set "$host_interface" master "$BRIDGE"
    ip link set "$host_interface" up
done
ip -n "$NETNS_A" addr add "$UNDERLAY_A/24" dev "$LINK_A"
ip -n "$NETNS_B" addr add "$UNDERLAY_B/24" dev "$LINK_B"
ip -n "$NETNS_C" addr add "$UNDERLAY_C/24" dev "$LINK_C"
ip -n "$NETNS_A" link set "$LINK_A" up
ip -n "$NETNS_B" link set "$LINK_B" up
ip -n "$NETNS_C" link set "$LINK_C" up

ip netns exec "$NETNS_A" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-a/agent.json" \
    >"$FIXTURE_ROOT/agent-a.log" 2>&1 &
AGENT_A_PID=$!
ip netns exec "$NETNS_B" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-b/agent.json" \
    >"$FIXTURE_ROOT/agent-b.log" 2>&1 &
AGENT_B_PID=$!
ip netns exec "$NETNS_C" "$AGENT" run \
    --config "$FIXTURE_ROOT/node-c/agent.json" \
    >"$FIXTURE_ROOT/agent-c.log" 2>&1 &
AGENT_C_PID=$!
wait_for_interface "$NETNS_A" xsa0
wait_for_interface "$NETNS_B" xsb0
wait_for_interface "$NETNS_C" xsc0
assert_agents_alive
sleep 1
assert_offline_configuration "$FIXTURE_ROOT/node-a/run/agent.sock"
assert_offline_configuration "$FIXTURE_ROOT/node-b/run/agent.sock"
assert_offline_configuration "$FIXTURE_ROOT/node-c/run/agent.sock"

ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination "$VIRTUAL_B" \
    --payload gate09-allowed-icmp \
    --sequence 1

ip netns exec "$NETNS_B" "$PROBE" tcp-server \
    --bind "$VIRTUAL_B" \
    --port 43311 \
    --request gate09-allowed-tcp-request \
    --response gate09-allowed-tcp-response \
    >"$FIXTURE_ROOT/allowed-tcp-server.log" 2>&1 &
allowed_tcp_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" tcp-client \
    --destination "$VIRTUAL_B" \
    --port 43311 \
    --request gate09-allowed-tcp-request \
    --response gate09-allowed-tcp-response
wait "$allowed_tcp_pid"

ip netns exec "$NETNS_B" "$PROBE" udp-server \
    --bind "$VIRTUAL_B" \
    --port 43313 \
    --request gate09-allowed-udp-request \
    --response gate09-allowed-udp-response \
    >"$FIXTURE_ROOT/allowed-udp-server.log" 2>&1 &
allowed_udp_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" udp-client \
    --destination "$VIRTUAL_B" \
    --port 43313 \
    --request gate09-allowed-udp-request \
    --response gate09-allowed-udp-response
wait "$allowed_udp_pid"

assert_icmp_denied "$NETNS_A" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_C" \
    "$VIRTUAL_C" 2 a-to-c-icmp-denied
assert_tcp_denied "$NETNS_A" "$NETNS_C" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_C" \
    "$VIRTUAL_C" 43311 a-to-c-tcp-denied
assert_udp_denied "$NETNS_A" "$NETNS_C" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_C" \
    "$VIRTUAL_A" "$VIRTUAL_C" 43313 a-to-c-udp-denied

assert_icmp_denied "$NETNS_C" "$LINK_C" "$UNDERLAY_C" "$UNDERLAY_B" \
    "$VIRTUAL_B" 3 c-to-b-icmp-denied
assert_tcp_denied "$NETNS_C" "$NETNS_B" "$LINK_C" "$UNDERLAY_C" "$UNDERLAY_B" \
    "$VIRTUAL_B" 43311 c-to-b-tcp-denied
assert_udp_denied "$NETNS_C" "$NETNS_B" "$LINK_C" "$UNDERLAY_C" "$UNDERLAY_B" \
    "$VIRTUAL_C" "$VIRTUAL_B" 43313 c-to-b-udp-denied

assert_tcp_denied "$NETNS_A" "$NETNS_B" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_B" \
    "$VIRTUAL_B" 43312 unexpected-tcp-port-denied
assert_udp_denied "$NETNS_A" "$NETNS_B" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_B" \
    "$VIRTUAL_A" "$VIRTUAL_B" 43314 unexpected-udp-port-denied

assert_udp_denied "$NETNS_C" "$NETNS_B" "$LINK_C" "$UNDERLAY_C" "$UNDERLAY_B" \
    "$VIRTUAL_A" "$VIRTUAL_B" 43313 forged-source-denied
assert_udp_denied "$NETNS_A" "$NETNS_B" "$LINK_A" "$UNDERLAY_A" "$UNDERLAY_B" \
    "$VIRTUAL_C" "$VIRTUAL_B" 43313 allowed-node-route-denied

assert_receiver_tcp_denied
assert_receiver_udp_denied

assert_agents_alive
assert_offline_configuration "$FIXTURE_ROOT/node-a/run/agent.sock"
assert_offline_configuration "$FIXTURE_ROOT/node-b/run/agent.sock"
assert_offline_configuration "$FIXTURE_ROOT/node-c/run/agent.sock"
if [[ -s $FIXTURE_ROOT/agent-a.log || -s $FIXTURE_ROOT/agent-b.log \
    || -s $FIXTURE_ROOT/agent-c.log ]]; then
    cat "$FIXTURE_ROOT/agent-a.log" "$FIXTURE_ROOT/agent-b.log" \
        "$FIXTURE_ROOT/agent-c.log" >&2
    exit 1
fi
printf 'Agent three-node ACL namespace tests passed\n'
