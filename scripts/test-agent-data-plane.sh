#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m13.XXXXXX)
NETNS_A="xsm13a-$$"
NETNS_B="xsm13b-$$"
VETH_A=xsm13va
VETH_B=xsm13vb
AGENT_A_PID=
AGENT_B_PID=
CONTROLLER_A_PID=
CONTROLLER_B_PID=

cleanup() {
    local status=$?
    set +e
    if ip netns list | awk '{print $1}' | grep -Fxq "$NETNS_B"; then
        ip netns exec "$NETNS_B" nft delete table inet xsm13 >/dev/null 2>&1
    fi
    for pid in "$CONTROLLER_A_PID" "$CONTROLLER_B_PID" "$AGENT_A_PID" "$AGENT_B_PID"; do
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

require_command() {
    if ! command -v "$1" >/dev/null; then
        printf 'required command is unavailable: %s\n' "$1" >&2
        exit 2
    fi
}

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

run_icmp_pair() {
    ip netns exec "$NETNS_A" "$PROBE" icmp \
        --destination 100.127.253.2 \
        --payload xs-m13-icmp-a-to-b \
        --sequence 1
    ip netns exec "$NETNS_B" "$PROBE" icmp \
        --destination 100.127.253.1 \
        --payload xs-m13-icmp-b-to-a \
        --sequence 2
}

if [[ $(id -u) -ne 0 ]]; then
    printf 'privileged Agent data-plane tests require root\n' >&2
    exit 2
fi
if [[ ! -c /dev/net/tun ]]; then
    printf '/dev/net/tun is unavailable\n' >&2
    exit 2
fi
for command in cargo ip nft python3 ss; do
    require_command "$command"
done
if [[ -e /sys/class/net/xsa0 || -e /sys/class/net/xsb0 ]]; then
    printf 'refusing to run while host test TUN interfaces exist\n' >&2
    exit 2
fi
if ip netns list | awk '{print $1}' | grep -Fxq "$NETNS_A" ||
    ip netns list | awk '{print $1}' | grep -Fxq "$NETNS_B"; then
    printf 'refusing to reuse an existing test namespace\n' >&2
    exit 2
fi

cd "$ROOT_DIR"
umask 077
python3 -m py_compile "$PROBE"
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo run -q -p xs-agent --features privileged-network-tests \
    --example generate_pair_fixture -- \
    "$FIXTURE_ROOT" \
    10.203.0.1:42001 \
    10.203.0.2:42001 >"$FIXTURE_ROOT/manifest.json"

ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip link add "$VETH_A" type veth peer name "$VETH_B"
ip link set "$VETH_A" netns "$NETNS_A"
ip link set "$VETH_B" netns "$NETNS_B"
ip -n "$NETNS_A" link set lo up
ip -n "$NETNS_B" link set lo up
ip -n "$NETNS_A" addr add 10.203.0.1/30 dev "$VETH_A"
ip -n "$NETNS_B" addr add 10.203.0.2/30 dev "$VETH_B"
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

ip -n "$NETNS_A" -brief addr show xsa0 | grep -F '100.127.253.1/32'
ip -n "$NETNS_B" -brief addr show xsb0 | grep -F '100.127.253.2/32'
ip netns exec "$NETNS_A" ss -lunp | grep -F '10.203.0.1:42001'
ip netns exec "$NETNS_B" ss -lunp | grep -F '10.203.0.2:42001'

run_icmp_pair

ip netns exec "$NETNS_B" "$PROBE" tcp-server \
    --bind 100.127.253.2 \
    --port 43101 \
    --request xs-m13-tcp-request \
    --response xs-m13-tcp-response \
    >"$FIXTURE_ROOT/tcp-server.log" 2>&1 &
tcp_server_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" tcp-client \
    --destination 100.127.253.2 \
    --port 43101 \
    --request xs-m13-tcp-request \
    --response xs-m13-tcp-response
wait "$tcp_server_pid"
cat "$FIXTURE_ROOT/tcp-server.log"

ip netns exec "$NETNS_A" "$PROBE" udp-server \
    --bind 100.127.253.1 \
    --port 43102 \
    --request xs-m13-udp-request \
    --response xs-m13-udp-response \
    >"$FIXTURE_ROOT/udp-server.log" 2>&1 &
udp_server_pid=$!
sleep 0.2
ip netns exec "$NETNS_B" "$PROBE" udp-client \
    --destination 100.127.253.1 \
    --port 43102 \
    --request xs-m13-udp-request \
    --response xs-m13-udp-response
wait "$udp_server_pid"
cat "$FIXTURE_ROOT/udp-server.log"

ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43103 \
    --expected xs-m13-encrypted-payload \
    --count 1 \
    --timeout 2 \
    >"$FIXTURE_ROOT/encrypted-server.log" 2>&1 &
encrypted_server_pid=$!
ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
    --interface "$VETH_A" \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --count 1 \
    --timeout 3 \
    --output "$FIXTURE_ROOT/encrypted-xsp.bin" \
    --metadata "$FIXTURE_ROOT/encrypted-xsp.json" \
    --forbid-text xs-m13-encrypted-payload \
    --forbid-file "$FIXTURE_ROOT/inner-ip.bin" \
    >"$FIXTURE_ROOT/encrypted-capture.log" 2>&1 &
encrypted_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.1 \
    --destination 100.127.253.2 \
    --source-port 53103 \
    --destination-port 43103 \
    --payload xs-m13-encrypted-payload \
    --packet-id 103 \
    --output "$FIXTURE_ROOT/inner-ip.bin"
wait "$encrypted_capture_pid"
wait "$encrypted_server_pid"
cat "$FIXTURE_ROOT/encrypted-capture.log"
cat "$FIXTURE_ROOT/encrypted-server.log"

ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43104 \
    --expected xs-m13-epoch-payload \
    --count 8 \
    --timeout 5 \
    >"$FIXTURE_ROOT/epoch-server.log" 2>&1 &
epoch_server_pid=$!
ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
    --interface "$VETH_A" \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --count 8 \
    --timeout 5 \
    --metadata "$FIXTURE_ROOT/epochs.json" \
    >"$FIXTURE_ROOT/epoch-capture.log" 2>&1 &
epoch_capture_pid=$!
sleep 0.2
for packet_id in $(seq 201 208); do
    ip netns exec "$NETNS_A" "$PROBE" send-virtual \
        --source 100.127.253.1 \
        --destination 100.127.253.2 \
        --source-port 53104 \
        --destination-port 43104 \
        --payload xs-m13-epoch-payload \
        --packet-id "$packet_id"
    sleep 0.2
done
wait "$epoch_capture_pid"
wait "$epoch_server_pid"
python3 - "$FIXTURE_ROOT/epochs.json" <<'PY'
import json
import sys

records = json.load(open(sys.argv[1], encoding="utf-8"))
epochs = sorted({record["epoch"] for record in records})
if len(epochs) < 2 or epochs[-1] - epochs[0] != len(epochs) - 1:
    raise SystemExit(f"automatic Key Epoch rotation was not observed: {epochs}")
print(f"key-epoch-ok epochs={epochs}")
PY
cat "$FIXTURE_ROOT/epoch-server.log"

ip netns exec "$NETNS_B" nft add table inet xsm13
ip netns exec "$NETNS_B" nft add chain inet xsm13 input \
    '{ type filter hook input priority 0; policy accept; }'
ip netns exec "$NETNS_B" nft add rule inet xsm13 input \
    ip saddr 10.203.0.1 udp dport 42001 drop
ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43105 \
    --expected xs-m13-tamper-replay \
    --count 1 \
    --timeout 3 \
    >"$FIXTURE_ROOT/tamper-server.log" 2>&1 &
tamper_server_pid=$!
ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
    --interface "$VETH_A" \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --count 1 \
    --timeout 3 \
    --output "$FIXTURE_ROOT/lost-xsp.bin" \
    >"$FIXTURE_ROOT/lost-capture.log" 2>&1 &
lost_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.1 \
    --destination 100.127.253.2 \
    --source-port 53105 \
    --destination-port 43105 \
    --payload xs-m13-tamper-replay \
    --packet-id 305
wait "$lost_capture_pid"
ip netns exec "$NETNS_B" nft delete table inet xsm13
ip netns exec "$NETNS_A" "$PROBE" inject-xsp \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --input "$FIXTURE_ROOT/lost-xsp.bin" \
    --packet-id 306 \
    --flip-last
sleep 0.3
kill -0 "$tamper_server_pid"
ip netns exec "$NETNS_A" "$PROBE" inject-xsp \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --input "$FIXTURE_ROOT/lost-xsp.bin" \
    --packet-id 307
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" inject-xsp \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --input "$FIXTURE_ROOT/lost-xsp.bin" \
    --packet-id 308
wait "$tamper_server_pid"
cat "$FIXTURE_ROOT/tamper-server.log"

ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind 100.127.253.2 \
    --port 43106 \
    --expected xs-m13-forged-source \
    --count 0 \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/forged-server.log" 2>&1 &
forged_server_pid=$!
ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$VETH_A" \
    --source 10.203.0.1 \
    --destination 10.203.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 \
    >"$FIXTURE_ROOT/forged-capture.log" 2>&1 &
forged_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.99 \
    --destination 100.127.253.2 \
    --source-port 53106 \
    --destination-port 43106 \
    --payload xs-m13-forged-source \
    --packet-id 406
wait "$forged_capture_pid"
wait "$forged_server_pid"
cat "$FIXTURE_ROOT/forged-capture.log"
cat "$FIXTURE_ROOT/forged-server.log"
assert_agents_alive

ip netns exec "$NETNS_A" python3 -m http.server 9 \
    --bind 127.0.0.1 >"$FIXTURE_ROOT/controller-a.log" 2>&1 &
CONTROLLER_A_PID=$!
ip netns exec "$NETNS_B" python3 -m http.server 9 \
    --bind 127.0.0.1 >"$FIXTURE_ROOT/controller-b.log" 2>&1 &
CONTROLLER_B_PID=$!
sleep 1
run_icmp_pair
kill "$CONTROLLER_A_PID" "$CONTROLLER_B_PID"
wait "$CONTROLLER_A_PID" "$CONTROLLER_B_PID" || true
CONTROLLER_A_PID=
CONTROLLER_B_PID=
sleep 1
run_icmp_pair
assert_agents_alive

if [[ -s $FIXTURE_ROOT/agent-a.log || -s $FIXTURE_ROOT/agent-b.log ]]; then
    cat "$FIXTURE_ROOT/agent-a.log" "$FIXTURE_ROOT/agent-b.log" >&2
    exit 1
fi
printf 'Agent XSP/1 two-node data-plane tests passed\n'
