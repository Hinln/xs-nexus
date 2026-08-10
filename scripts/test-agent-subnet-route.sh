#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m32-subnet.XXXXXX)
NETNS_A="xsm32a-$$"
NETNS_B="xsm32b-$$"
NETNS_LAN="xsm32l-$$"
UNDERLAY_A=xsm32ua
UNDERLAY_B=xsm32ub
LAN_B=xsm32lanb
LAN_HOST=xsm32lanh
AGENT_A_PID=
AGENT_B_PID=
FORWARD_LAN_BEFORE=

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
    stop_agents
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    ip netns del "$NETNS_LAN" >/dev/null 2>&1
    rm -rf "$FIXTURE_ROOT"
    exit "$status"
}
trap report_failure ERR
trap cleanup EXIT INT TERM

stop_agents() {
    local pid
    for pid in "$AGENT_A_PID" "$AGENT_B_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
        fi
    done
    for pid in "$AGENT_A_PID" "$AGENT_B_PID"; do
        if [[ -n $pid ]]; then
            wait "$pid" >/dev/null 2>&1
        fi
    done
    AGENT_A_PID=
    AGENT_B_PID=
}

generate_fixture() {
    local mode=$1
    local candidate_ttl=$2
    XS_FIXTURE_ACL_MODE="$mode" \
    XS_FIXTURE_CANDIDATE_TTL_SECONDS="$candidate_ttl" \
        cargo run -q -p xs-agent --features privileged-network-tests \
        --example generate_pair_fixture -- \
        "$FIXTURE_ROOT" \
        10.232.0.1:42001 \
        10.232.0.2:42001 >"$FIXTURE_ROOT/manifest.json"
}

start_agents() {
    : >"$FIXTURE_ROOT/agent-a.log"
    : >"$FIXTURE_ROOT/agent-b.log"
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
    kill -0 "$AGENT_A_PID"
    kill -0 "$AGENT_B_PID"
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

wait_for_route() {
    for _ in $(seq 1 80); do
        if ip -n "$NETNS_A" route show 192.168.232.0/24 dev xsa0 | grep -q '^192\.168\.232\.0/24'; then
            return
        fi
        sleep 0.1
    done
    printf 'timed out waiting for approved subnet route\n' >&2
    exit 1
}

wait_for_route_removal() {
    for _ in $(seq 1 250); do
        if ! ip -n "$NETNS_A" route show 192.168.232.0/24 | grep -q .; then
            return
        fi
        sleep 0.1
    done
    printf 'expired gateway route remained installed\n' >&2
    exit 1
}

wait_for_gateway_resources() {
    local expect_nat=$1
    for _ in $(seq 1 80); do
        if [[ $(ip netns exec "$NETNS_B" cat /proc/sys/net/ipv4/conf/xsb0/forwarding) == 1 \
            && $(ip netns exec "$NETNS_B" cat "/proc/sys/net/ipv4/conf/$LAN_B/forwarding") == 1 ]]; then
            if [[ $expect_nat == yes ]] && ip netns exec "$NETNS_B" nft list table ip xs_nexus_xsb0 >/dev/null 2>&1; then
                return
            fi
            if [[ $expect_nat == no ]] && ! ip netns exec "$NETNS_B" nft list table ip xs_nexus_xsb0 >/dev/null 2>&1; then
                return
            fi
        fi
        sleep 0.1
    done
    printf 'timed out waiting for gateway forwarding/NAT resources\n' >&2
    exit 1
}

assert_gateway_restored() {
    if [[ $(ip netns exec "$NETNS_B" cat "/proc/sys/net/ipv4/conf/$LAN_B/forwarding") != "$FORWARD_LAN_BEFORE" ]]; then
        printf 'gateway LAN forwarding sysctl was not restored\n' >&2
        exit 1
    fi
    if ip netns exec "$NETNS_B" nft list table ip xs_nexus_xsb0 >/dev/null 2>&1; then
        printf 'project nftables table remained after gateway shutdown\n' >&2
        exit 1
    fi
}

run_tcp_probe() {
    local label=$1
    local port=$2
    ip netns exec "$NETNS_LAN" "$PROBE" tcp-server \
        --bind 192.168.232.2 \
        --port "$port" \
        --request "$label-request" \
        --response "$label-response" \
        >"$FIXTURE_ROOT/$label-server.log" 2>&1 &
    local server_pid=$!
    sleep 0.2
    ip netns exec "$NETNS_A" "$PROBE" tcp-client \
        --destination 192.168.232.2 \
        --port "$port" \
        --request "$label-request" \
        --response "$label-response" \
        --timeout 3
    wait "$server_pid"
}

dump_diagnostics() {
    set +e
    for namespace in "$NETNS_A" "$NETNS_B" "$NETNS_LAN"; do
        printf '%s\n' "--- namespace $namespace routes ---" >&2
        ip -n "$namespace" -4 route show table all >&2
        printf '%s\n' "--- namespace $namespace links ---" >&2
        ip -n "$namespace" -s link show >&2
    done
    printf '%s\n' '--- gateway sysctls ---' >&2
    ip netns exec "$NETNS_B" sysctl net.ipv4.ip_forward \
        net.ipv4.conf.all.rp_filter \
        net.ipv4.conf.xsb0.forwarding \
        "net.ipv4.conf.$LAN_B.forwarding" >&2
    printf '%s\n' '--- gateway nftables ---' >&2
    ip netns exec "$NETNS_B" nft list ruleset >&2
    printf '%s\n' '--- Agent peers ---' >&2
    "$ROOT_DIR/target/debug/xs" peers \
        --socket "$FIXTURE_ROOT/node-a/run/agent.sock" --json >&2
    "$ROOT_DIR/target/debug/xs" peers \
        --socket "$FIXTURE_ROOT/node-b/run/agent.sock" --json >&2
    cat "$FIXTURE_ROOT/agent-a.log" "$FIXTURE_ROOT/agent-b.log" >&2
}

if [[ $(id -u) -ne 0 || ! -c /dev/net/tun ]]; then
    printf 'privileged subnet route tests require root and /dev/net/tun\n' >&2
    exit 2
fi
for command in cargo ip nft python3; do
    command -v "$command" >/dev/null
done
if [[ -e /sys/class/net/xsa0 || -e /sys/class/net/xsb0 ]]; then
    printf 'refusing to run while host test TUN interfaces exist\n' >&2
    exit 2
fi

cd "$ROOT_DIR"
umask 077
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli

ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip netns add "$NETNS_LAN"
ip link add "$UNDERLAY_A" type veth peer name "$UNDERLAY_B"
ip link set "$UNDERLAY_A" netns "$NETNS_A"
ip link set "$UNDERLAY_B" netns "$NETNS_B"
ip link add "$LAN_B" type veth peer name "$LAN_HOST"
ip link set "$LAN_B" netns "$NETNS_B"
ip link set "$LAN_HOST" netns "$NETNS_LAN"
for namespace in "$NETNS_A" "$NETNS_B" "$NETNS_LAN"; do
    ip -n "$namespace" link set lo up
done
ip -n "$NETNS_A" addr add 10.232.0.1/30 dev "$UNDERLAY_A"
ip -n "$NETNS_B" addr add 10.232.0.2/30 dev "$UNDERLAY_B"
ip -n "$NETNS_B" addr add 192.168.232.1/24 dev "$LAN_B"
ip -n "$NETNS_LAN" addr add 192.168.232.2/24 dev "$LAN_HOST"
ip -n "$NETNS_A" link set "$UNDERLAY_A" up
ip -n "$NETNS_B" link set "$UNDERLAY_B" up
ip -n "$NETNS_B" link set "$LAN_B" up
ip -n "$NETNS_LAN" link set "$LAN_HOST" up
ip netns exec "$NETNS_B" sysctl -q -w net.ipv4.conf.all.rp_filter=0
ip netns exec "$NETNS_B" sysctl -q -w "net.ipv4.conf.$LAN_B.rp_filter=0"
ip netns exec "$NETNS_LAN" sysctl -q -w net.ipv4.conf.all.rp_filter=0
ip netns exec "$NETNS_LAN" sysctl -q -w "net.ipv4.conf.$LAN_HOST.rp_filter=0"
FORWARD_LAN_BEFORE=$(ip netns exec "$NETNS_B" cat "/proc/sys/net/ipv4/conf/$LAN_B/forwarding")

generate_fixture subnet_none 60
start_agents
if ip -n "$NETNS_A" route show 192.168.232.0/24 | grep -q .; then
    printf 'unapproved subnet route was installed\n' >&2
    exit 1
fi
if ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 192.168.232.2 \
    --payload xs-m32-before-approval \
    --sequence 1 \
    --timeout 1 >"$FIXTURE_ROOT/before-approval.log" 2>&1; then
    printf 'subnet was reachable before approval\n' >&2
    exit 1
fi
stop_agents
assert_gateway_restored

generate_fixture subnet_routed 60
ip -n "$NETNS_LAN" route replace 100.127.253.0/24 via 192.168.232.1 dev "$LAN_HOST"
start_agents
wait_for_route
wait_for_gateway_resources no
if ! ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 192.168.232.2 \
    --payload xs-m32-routed-icmp \
    --sequence 2 \
    --timeout 3; then
    dump_diagnostics
    exit 1
fi
run_tcp_probe xs-m32-routed 43221

ip netns exec "$NETNS_LAN" "$PROBE" tcp-server \
    --bind 192.168.232.2 \
    --port 43222 \
    --request xs-gate09-subnet-tcp-denied-request \
    --response xs-gate09-subnet-tcp-denied-response \
    --timeout 1.5 >"$FIXTURE_ROOT/subnet-tcp-denied-server.log" 2>&1 &
subnet_tcp_denied_pid=$!
ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$UNDERLAY_A" \
    --source 10.232.0.1 \
    --destination 10.232.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 >"$FIXTURE_ROOT/subnet-tcp-denied-capture.log" 2>&1 &
subnet_tcp_denied_capture_pid=$!
sleep 0.2
if ip netns exec "$NETNS_A" "$PROBE" tcp-client \
    --destination 192.168.232.2 \
    --port 43222 \
    --request xs-gate09-subnet-tcp-denied-request \
    --response xs-gate09-subnet-tcp-denied-response \
    --timeout 1 >"$FIXTURE_ROOT/subnet-tcp-denied-client.log" 2>&1; then
    printf 'subnet route bypassed the TCP ACL\n' >&2
    exit 1
fi
wait "$subnet_tcp_denied_capture_pid"
if wait "$subnet_tcp_denied_pid"; then
    printf 'ACL-denied TCP reached the routed subnet\n' >&2
    exit 1
fi

ip netns exec "$NETNS_LAN" "$PROBE" collect-udp \
    --bind 192.168.232.2 \
    --port 43224 \
    --expected xs-gate09-subnet-udp-denied \
    --count 0 \
    --timeout 1.5 >"$FIXTURE_ROOT/subnet-udp-denied-server.log" 2>&1 &
subnet_udp_denied_pid=$!
ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$UNDERLAY_A" \
    --source 10.232.0.1 \
    --destination 10.232.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 >"$FIXTURE_ROOT/subnet-udp-denied-capture.log" 2>&1 &
subnet_udp_denied_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.1 \
    --destination 192.168.232.2 \
    --source-port 53224 \
    --destination-port 43224 \
    --payload xs-gate09-subnet-udp-denied \
    --packet-id 324
wait "$subnet_udp_denied_capture_pid"
wait "$subnet_udp_denied_pid"

ip netns exec "$NETNS_A" "$PROBE" assert-no-xsp \
    --interface "$UNDERLAY_A" \
    --source 10.232.0.1 \
    --destination 10.232.0.2 \
    --source-port 42001 \
    --destination-port 42001 \
    --packet-type data \
    --timeout 1.5 >"$FIXTURE_ROOT/spoof-capture.log" 2>&1 &
spoof_capture_pid=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source 100.127.253.99 \
    --destination 192.168.232.2 \
    --source-port 53222 \
    --destination-port 43222 \
    --payload xs-m32-spoof-denied \
    --packet-id 322
wait "$spoof_capture_pid"

stop_agents
assert_gateway_restored
if [[ $(ip netns exec "$NETNS_B" cat "/proc/sys/net/ipv4/conf/$LAN_B/forwarding") != "$FORWARD_LAN_BEFORE" ]]; then
    printf 'routed mode did not restore forwarding\n' >&2
    exit 1
fi

ip -n "$NETNS_LAN" route del 100.127.253.0/24
generate_fixture subnet_nat 12
start_agents
wait_for_route
wait_for_gateway_resources yes
nft_rules=$(ip netns exec "$NETNS_B" nft list table ip xs_nexus_xsb0)
for expected in 'iifname "xsb0"' "oifname \"$LAN_B\"" 'ip daddr 192.168.232.0/24' masquerade; do
    if [[ $nft_rules != *"$expected"* ]]; then
        printf 'missing scoped NAT expression: %s\n' "$expected" >&2
        exit 1
    fi
done
run_tcp_probe xs-m32-nat 43223

kill "$AGENT_B_PID"
wait "$AGENT_B_PID"
AGENT_B_PID=
assert_gateway_restored
wait_for_route_removal
kill -0 "$AGENT_A_PID"
if ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination 192.168.232.2 \
    --payload xs-m32-gateway-offline \
    --sequence 3 \
    --timeout 1 >"$FIXTURE_ROOT/gateway-offline.log" 2>&1; then
    printf 'subnet remained reachable after gateway expiry\n' >&2
    exit 1
fi
stop_agents

if [[ -s $FIXTURE_ROOT/agent-a.log || -s $FIXTURE_ROOT/agent-b.log ]]; then
    cat "$FIXTURE_ROOT/agent-a.log" "$FIXTURE_ROOT/agent-b.log" >&2
    exit 1
fi
printf 'Agent subnet route namespace tests passed\n'
