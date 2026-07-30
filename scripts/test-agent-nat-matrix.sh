#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
FIXTURE_ROOT=$(mktemp -d /tmp/xs-m22-nat.XXXXXX)
SUFFIX=$(printf '%04x' "$(( $$ % 65536 ))")
AGENT_A_PID=
AGENT_B_PID=
CASE_ROOT=
NETNS_NAMES=()
REBOUND_ADDRESS=

cleanup_case() {
    local status=${1:-0}
    set +e
    for pid in "$AGENT_A_PID" "$AGENT_B_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
            wait "$pid" >/dev/null 2>&1
        fi
    done
    AGENT_A_PID=
    AGENT_B_PID=
    for namespace in "${NETNS_NAMES[@]}"; do
        ip netns del "$namespace" >/dev/null 2>&1
    done
    if (( status != 0 )) && [[ -n $CASE_ROOT ]]; then
        for log in agent-a.log agent-b.log; do
            if [[ -s $CASE_ROOT/$log ]]; then
                printf '\n--- %s ---\n' "$log" >&2
                cat "$CASE_ROOT/$log" >&2
            fi
        done
    fi
    NETNS_NAMES=()
    CASE_ROOT=
    set -e
}

cleanup() {
    local status=$?
    cleanup_case "$status"
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
    for _ in $(seq 1 120); do
        if ip netns exec "$namespace" ip link show dev "$interface" >/dev/null 2>&1; then
            return
        fi
        sleep 0.1
    done
    printf 'timed out waiting for %s in %s\n' "$interface" "$namespace" >&2
    exit 1
}

session_ready() {
    local socket=$1
    local response
    response=$($CLI peers --socket "$socket" --json 2>/dev/null || true)
    [[ -n $response ]] && python3 - "$response" <<'PY'
import json
import sys

response = json.loads(sys.argv[1])
raise SystemExit(not response["peers"][0]["session_established"])
PY
}

wait_for_direct_session() {
    local socket=$1
    local expected_address=$2
    local expected_reason=${3:-}
    for _ in $(seq 1 240); do
        local response
        response=$($CLI peers --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]] && python3 - "$expected_address" "$expected_reason" "$response" <<'PY'
import json
import sys

expected_address = sys.argv[1]
expected_reason = sys.argv[2]
response = json.loads(sys.argv[3])
peer = response["peers"][0]
endpoint = peer.get("active_endpoint")
ready = (
    peer["session_established"]
    and endpoint
    and endpoint.rsplit(":", 1)[0] == expected_address
    and (not expected_reason or peer.get("path_reason") == expected_reason)
)
raise SystemExit(not ready)
PY
        then
            return
        fi
        sleep 0.1
    done
    printf 'direct session did not become ready: %s expected=%s\n' \
        "$socket" "$expected_address" >&2
    $CLI peers --socket "$socket" --json >&2 || true
    exit 1
}

assert_no_direct_session() {
    local socket_a=$1
    local socket_b=$2
    local duration=$3
    local attempts
    attempts=$(( duration * 10 ))
    for _ in $(seq 1 "$attempts"); do
        if session_ready "$socket_a" || session_ready "$socket_b"; then
            printf 'unexpected direct session became ready\n' >&2
            $CLI peers --socket "$socket_a" --json >&2 || true
            $CLI peers --socket "$socket_b" --json >&2 || true
            exit 1
        fi
        sleep 0.1
    done
}

nat_packet_count() {
    local router=$1
    ip netns exec "$router" nft list chain inet xsfilter forward |
        awk '
            /iifname "lan0".*oifname "wan0".*counter packets/ {
                for (field = 1; field <= NF; field++) {
                    if ($field == "packets") {
                        print $(field + 1)
                        exit
                    }
                }
            }
        '
}

configure_filter() {
    local router=$1
    local private_address=$2
    local peer_public_address=$3
    local mode=$4
    local peer_source_port=$5

    ip netns exec "$router" nft flush chain inet xsfilter forward
    if [[ $mode == blocked ]]; then
        return
    fi
    ip netns exec "$router" nft add rule inet xsfilter forward \
        iifname lan0 oifname wan0 ip saddr "$private_address" \
        meta l4proto udp counter accept
    case "$mode" in
        full)
            ip netns exec "$router" nft add rule inet xsfilter forward \
                iifname wan0 oifname lan0 ip daddr "$private_address" udp dport 43001 \
                counter accept
            ;;
        symmetric)
            ;;
        restricted)
            ip netns exec "$router" nft add rule inet xsfilter forward \
                iifname wan0 oifname lan0 ip saddr "$peer_public_address" \
                ip daddr "$private_address" udp dport 43001 counter accept
            ;;
        port-restricted)
            ip netns exec "$router" nft add rule inet xsfilter forward \
                iifname wan0 oifname lan0 ip saddr "$peer_public_address" \
                ip daddr "$private_address" udp sport "$peer_source_port" \
                udp dport 43001 counter accept
            ;;
        *)
            printf 'unknown NAT filter mode: %s\n' "$mode" >&2
            exit 2
            ;;
    esac
}

configure_router() {
    local router=$1
    local private_address=$2
    local public_address=$3
    local peer_public_address=$4
    local source_port=$5
    local mode=$6
    local peer_source_port=$7

    ip netns exec "$router" sysctl -qw net.ipv4.ip_forward=1
    ip netns exec "$router" sysctl -qw net.ipv4.conf.all.rp_filter=0
    ip netns exec "$router" nft add table ip xsnat
    ip netns exec "$router" nft add chain ip xsnat prerouting \
        '{ type nat hook prerouting priority dstnat; policy accept; }'
    ip netns exec "$router" nft add chain ip xsnat postrouting \
        '{ type nat hook postrouting priority srcnat; policy accept; }'
    ip netns exec "$router" nft add rule ip xsnat prerouting \
        iifname wan0 ip daddr "$public_address" udp dport 43001 \
        counter dnat to "$private_address:43001"
    ip netns exec "$router" nft add rule ip xsnat postrouting \
        oifname wan0 ip saddr "$private_address" udp sport 43001 \
        counter snat to "$public_address:$source_port"
    ip netns exec "$router" nft add table inet xsfilter
    ip netns exec "$router" nft add chain inet xsfilter forward \
        '{ type filter hook forward priority filter; policy drop; }'
    configure_filter \
        "$router" \
        "$private_address" \
        "$peer_public_address" \
        "$mode" \
        "$peer_source_port"
}

rebind_router_a() {
    local old_router=$NS_RA
    local replacement_router="x22r6n${SUFFIX}"
    local new_public_address="198.18.6.9"
    local lan_link="m6na${SUFFIX}"
    local router_lan="m6nla${SUFFIX}"
    local wan_link="m6nwa${SUFFIX}"
    local wan_peer="m6npa${SUFFIX}"

    ip netns del "$old_router"
    ip netns add "$replacement_router"
    NETNS_NAMES=("$NS_A" "$NS_B" "$replacement_router" "$NS_RB" "$NS_WAN")
    NS_RA=$replacement_router
    ip -n "$NS_RA" link set lo up

    ip link add "$lan_link" type veth peer name "$router_lan"
    ip link set "$lan_link" netns "$NS_A"
    ip link set "$router_lan" netns "$NS_RA"
    ip -n "$NS_A" link set "$lan_link" name lan0
    ip -n "$NS_RA" link set "$router_lan" name lan0
    ip link add "$wan_link" type veth peer name "$wan_peer"
    ip link set "$wan_link" netns "$NS_RA"
    ip link set "$wan_peer" netns "$NS_WAN"
    ip -n "$NS_RA" link set "$wan_link" name wan0
    ip -n "$NS_WAN" link set "$wan_peer" name wa0
    ip -n "$NS_WAN" link set wa0 master br0
    ip -n "$NS_WAN" link set wa0 up
    ip -n "$NS_A" addr add "$PRIVATE_A/24" dev lan0
    ip -n "$NS_RA" addr add "$GATEWAY_A/24" dev lan0
    ip -n "$NS_RA" addr add "$new_public_address/24" dev wan0
    ip -n "$NS_A" link set lan0 up
    ip -n "$NS_RA" link set lan0 up
    ip -n "$NS_RA" link set wan0 up
    ip -n "$NS_A" route add default via "$GATEWAY_A"
    configure_router \
        "$NS_RA" "$PRIVATE_A" "$new_public_address" "$PUBLIC_B" \
        43001 full 43001
    REBOUND_ADDRESS=$new_public_address
}

create_topology() {
    local case_id=$1
    local mode_a=$2
    local mode_b=$3
    local source_port_a=$4
    local source_port_b=$5

    NS_A="x22a${case_id}${SUFFIX}"
    NS_B="x22b${case_id}${SUFFIX}"
    NS_RA="x22r${case_id}a${SUFFIX}"
    NS_RB="x22r${case_id}b${SUFFIX}"
    NS_WAN="x22w${case_id}${SUFFIX}"
    NETNS_NAMES=("$NS_A" "$NS_B" "$NS_RA" "$NS_RB" "$NS_WAN")
    PRIVATE_A="10.220.${case_id}.2"
    GATEWAY_A="10.220.${case_id}.1"
    PRIVATE_B="10.221.${case_id}.2"
    GATEWAY_B="10.221.${case_id}.1"
    PUBLIC_A="198.18.${case_id}.1"
    PUBLIC_B="198.18.${case_id}.2"

    for namespace in "${NETNS_NAMES[@]}"; do
        ip netns add "$namespace"
        ip -n "$namespace" link set lo up
    done

    local a_link="m${case_id}a${SUFFIX}"
    local ra_lan="m${case_id}rla${SUFFIX}"
    local b_link="m${case_id}b${SUFFIX}"
    local rb_lan="m${case_id}rlb${SUFFIX}"
    local ra_wan="m${case_id}rwa${SUFFIX}"
    local wan_a="m${case_id}wa${SUFFIX}"
    local rb_wan="m${case_id}rwb${SUFFIX}"
    local wan_b="m${case_id}wb${SUFFIX}"

    ip link add "$a_link" type veth peer name "$ra_lan"
    ip link add "$b_link" type veth peer name "$rb_lan"
    ip link add "$ra_wan" type veth peer name "$wan_a"
    ip link add "$rb_wan" type veth peer name "$wan_b"
    ip link set "$a_link" netns "$NS_A"
    ip link set "$ra_lan" netns "$NS_RA"
    ip link set "$b_link" netns "$NS_B"
    ip link set "$rb_lan" netns "$NS_RB"
    ip link set "$ra_wan" netns "$NS_RA"
    ip link set "$wan_a" netns "$NS_WAN"
    ip link set "$rb_wan" netns "$NS_RB"
    ip link set "$wan_b" netns "$NS_WAN"

    ip -n "$NS_A" link set "$a_link" name lan0
    ip -n "$NS_B" link set "$b_link" name lan0
    ip -n "$NS_RA" link set "$ra_lan" name lan0
    ip -n "$NS_RA" link set "$ra_wan" name wan0
    ip -n "$NS_RB" link set "$rb_lan" name lan0
    ip -n "$NS_RB" link set "$rb_wan" name wan0
    ip -n "$NS_WAN" link set "$wan_a" name wa0
    ip -n "$NS_WAN" link set "$wan_b" name wb0

    ip netns exec "$NS_WAN" ip link add br0 type bridge
    ip -n "$NS_WAN" link set wa0 master br0
    ip -n "$NS_WAN" link set wb0 master br0
    ip -n "$NS_WAN" link set br0 up
    ip -n "$NS_WAN" link set wa0 up
    ip -n "$NS_WAN" link set wb0 up

    ip -n "$NS_A" addr add "$PRIVATE_A/24" dev lan0
    ip -n "$NS_B" addr add "$PRIVATE_B/24" dev lan0
    ip -n "$NS_RA" addr add "$GATEWAY_A/24" dev lan0
    ip -n "$NS_RB" addr add "$GATEWAY_B/24" dev lan0
    ip -n "$NS_RA" addr add "$PUBLIC_A/24" dev wan0
    ip -n "$NS_RB" addr add "$PUBLIC_B/24" dev wan0
    for namespace in "$NS_A" "$NS_B" "$NS_RA" "$NS_RB"; do
        ip -n "$namespace" link set lan0 up
    done
    ip -n "$NS_RA" link set wan0 up
    ip -n "$NS_RB" link set wan0 up
    ip -n "$NS_A" route add default via "$GATEWAY_A"
    ip -n "$NS_B" route add default via "$GATEWAY_B"

    configure_router \
        "$NS_RA" "$PRIVATE_A" "$PUBLIC_A" "$PUBLIC_B" \
        "$source_port_a" "$mode_a" "$source_port_b"
    configure_router \
        "$NS_RB" "$PRIVATE_B" "$PUBLIC_B" "$PUBLIC_A" \
        "$source_port_b" "$mode_b" "$source_port_a"
}

start_agents() {
    local case_name=$1
    CASE_ROOT="$FIXTURE_ROOT/$case_name"
    mkdir -p "$CASE_ROOT"
    cargo run -q -p xs-agent --features privileged-network-tests \
        --example generate_pair_fixture -- \
        "$CASE_ROOT" \
        "$PRIVATE_A:43001" \
        "$PRIVATE_B:43001" \
        "$PUBLIC_A:43001" \
        "$PUBLIC_B:43001" >"$CASE_ROOT/manifest.json"
    ip netns exec "$NS_A" "$AGENT" run \
        --config "$CASE_ROOT/node-a/agent.json" >"$CASE_ROOT/agent-a.log" 2>&1 &
    AGENT_A_PID=$!
    ip netns exec "$NS_B" "$AGENT" run \
        --config "$CASE_ROOT/node-b/agent.json" >"$CASE_ROOT/agent-b.log" 2>&1 &
    AGENT_B_PID=$!
    wait_for_interface "$NS_A" xsa0
    wait_for_interface "$NS_B" xsb0
}

assert_nat_activity() {
    local packets_a
    local packets_b
    packets_a=$(nat_packet_count "$NS_RA")
    packets_b=$(nat_packet_count "$NS_RB")
    if [[ ! $packets_a =~ ^[1-9][0-9]*$ ]] || [[ ! $packets_b =~ ^[1-9][0-9]*$ ]]; then
        printf 'both peers did not actively punch: A=%s B=%s\n' \
            "$packets_a" "$packets_b" >&2
        exit 1
    fi
}

assert_bidirectional_icmp() {
    local label=$1
    ip netns exec "$NS_A" "$PROBE" icmp \
        --destination 100.127.253.2 \
        --payload "xs-m22-$label-a" \
        --sequence 1 \
        --timeout 3
    ip netns exec "$NS_B" "$PROBE" icmp \
        --destination 100.127.253.1 \
        --payload "xs-m22-$label-b" \
        --sequence 2 \
        --timeout 3
}

run_success_case() {
    local label=$1
    local case_id=$2
    local mode_a=$3
    local mode_b=$4
    printf 'NAT case: %s\n' "$label"
    create_topology "$case_id" "$mode_a" "$mode_b" 43001 43001
    start_agents "$label"
    wait_for_direct_session "$CASE_ROOT/node-a/run/agent.sock" "$PUBLIC_B"
    wait_for_direct_session "$CASE_ROOT/node-b/run/agent.sock" "$PUBLIC_A"
    assert_nat_activity
    assert_bidirectional_icmp "$label"
    cleanup_case 0
}

run_symmetric_case() {
    local label=symmetric-nat
    printf 'NAT case: %s\n' "$label"
    create_topology 4 symmetric full 44001 43001
    start_agents "$label"
    assert_no_direct_session \
        "$CASE_ROOT/node-a/run/agent.sock" \
        "$CASE_ROOT/node-b/run/agent.sock" \
        6
    assert_nat_activity
    cleanup_case 0
}

run_public_rebind_case() {
    local label=public-ip-rebind
    printf 'NAT case: %s\n' "$label"
    create_topology 6 full full 43001 43001
    start_agents "$label"
    wait_for_direct_session "$CASE_ROOT/node-a/run/agent.sock" "$PUBLIC_B"
    wait_for_direct_session "$CASE_ROOT/node-b/run/agent.sock" "$PUBLIC_A"
    rebind_router_a
    wait_for_direct_session \
        "$CASE_ROOT/node-b/run/agent.sock" \
        "$REBOUND_ADDRESS" \
        authenticated_peer_traffic
    assert_nat_activity
    assert_bidirectional_icmp "$label"
    cleanup_case 0
}

run_blocked_recovery_case() {
    local label=udp-blocked-recovery
    printf 'NAT case: %s\n' "$label"
    create_topology 5 blocked blocked 43001 43001
    start_agents "$label"
    assert_no_direct_session \
        "$CASE_ROOT/node-a/run/agent.sock" \
        "$CASE_ROOT/node-b/run/agent.sock" \
        4
    configure_filter "$NS_RA" "$PRIVATE_A" "$PUBLIC_B" full 43001
    configure_filter "$NS_RB" "$PRIVATE_B" "$PUBLIC_A" full 43001
    wait_for_direct_session "$CASE_ROOT/node-a/run/agent.sock" "$PUBLIC_B"
    wait_for_direct_session "$CASE_ROOT/node-b/run/agent.sock" "$PUBLIC_A"
    assert_nat_activity
    assert_bidirectional_icmp "$label"
    cleanup_case 0
}

if [[ $(id -u) -ne 0 ]] || [[ ! -c /dev/net/tun ]]; then
    printf 'NAT matrix requires root and /dev/net/tun\n' >&2
    exit 2
fi
for command in awk cargo ip nft python3 sysctl; do
    require_command "$command"
done

cd "$ROOT_DIR"
umask 077
python3 -m py_compile "$PROBE"
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli

scripts/test-agent-proactive-punch.sh
run_success_case full-cone-two-sided 1 full full
run_success_case restricted-two-sided 2 restricted restricted
run_success_case port-restricted-two-sided 3 port-restricted port-restricted
run_public_rebind_case
run_symmetric_case
run_blocked_recovery_case

printf 'Agent UDP NAT traversal matrix passed\n'
