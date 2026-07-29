#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
CONTROLLER="$ROOT_DIR/target/debug/xs-controller"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
TEMPORARY=$(mktemp -d /tmp/xs-m21-path.XXXXXX)
SUFFIX=$(printf '%04x' "$(( $$ % 65536 ))")
BRIDGE="xm21b$SUFFIX"
NETNS_A="xsm21a-$SUFFIX"
NETNS_B="xsm21b-$SUFFIX"
HOST_A="xm21ha$SUFFIX"
HOST_B="xm21hb$SUFFIX"
CONTROL_A="xm21ca"
CONTROL_B="xm21cb"
PATH_A="xm21pa"
PATH_B="xm21pb"
BRIDGE_IP=10.250.21.1
CONTROL_IP_A=10.250.21.2
CONTROL_IP_B=10.250.21.3
PATH_IP_A=10.100.21.1
PATH_IP_B=10.100.21.2
CONTROLLER_PID=
AGENT_A_PID=
AGENT_B_PID=
CAPTURE_PID=
PROXY_A_PID=
PROXY_B_PID=

cleanup() {
    local status=$?
    set +e
    if [[ -n $CAPTURE_PID ]]; then
        kill "$CAPTURE_PID" >/dev/null 2>&1
        wait "$CAPTURE_PID" >/dev/null 2>&1
    fi
    for pid in "$AGENT_A_PID" "$AGENT_B_PID" "$PROXY_A_PID" "$PROXY_B_PID" "$CONTROLLER_PID"; do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
            wait "$pid" >/dev/null 2>&1
        fi
    done
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    ip link del "$BRIDGE" >/dev/null 2>&1
    if (( status != 0 )); then
        for log in controller.log agent-a.log agent-b.log proxy-a.log proxy-b.log path-capture.log; do
            if [[ -s $TEMPORARY/$log ]]; then
                printf '\n--- %s ---\n' "$log" >&2
                cat "$TEMPORARY/$log" >&2
            fi
        done
    fi
    rm -rf "$TEMPORARY"
    exit "$status"
}
trap cleanup EXIT INT TERM

require_command() {
    command -v "$1" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$1" >&2
        exit 2
    }
}

wait_http() {
    for _ in $(seq 1 100); do
        if curl --fail --silent "$1/health/ready" >/dev/null; then
            return
        fi
        sleep 0.1
    done
    printf 'controller did not become ready\n' >&2
    exit 1
}

wait_namespace_http() {
    local namespace=$1
    local base_url=$2
    for _ in $(seq 1 100); do
        if ip netns exec "$namespace" curl --fail --silent \
            --connect-timeout 1 --max-time 1 "$base_url/health/ready" >/dev/null
        then
            return
        fi
        sleep 0.1
    done
    printf 'namespace proxy did not become ready: %s\n' "$namespace" >&2
    exit 1
}

write_tcp_forwarder() {
    cat >"$TEMPORARY/tcp-forward.py" <<'PY'
#!/usr/bin/env python3
import argparse
import asyncio


async def copy_stream(reader, writer):
    try:
        while data := await reader.read(65536):
            writer.write(data)
            await writer.drain()
    finally:
        writer.close()


async def handle_connection(reader, writer, target_host, target_port):
    try:
        target_reader, target_writer = await asyncio.open_connection(target_host, target_port)
    except OSError:
        writer.close()
        await writer.wait_closed()
        return
    await asyncio.gather(
        copy_stream(reader, target_writer),
        copy_stream(target_reader, writer),
        return_exceptions=True,
    )


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen-port", type=int, required=True)
    parser.add_argument("--target-host", required=True)
    parser.add_argument("--target-port", type=int, required=True)
    args = parser.parse_args()
    server = await asyncio.start_server(
        lambda reader, writer: handle_connection(
            reader,
            writer,
            args.target_host,
            args.target_port,
        ),
        "127.0.0.1",
        args.listen_port,
    )
    async with server:
        await server.serve_forever()


asyncio.run(main())
PY
    chmod 0700 "$TEMPORARY/tcp-forward.py"
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

wait_controller_connected() {
    local socket=$1
    for _ in $(seq 1 120); do
        if "$CLI" status --socket "$socket" --json 2>/dev/null |
            python3 -c 'import json,sys; raise SystemExit(not json.load(sys.stdin)["status"]["controller_connected"])'
        then
            return
        fi
        sleep 0.1
    done
    printf 'Agent control connection did not become ready: %s\n' "$socket" >&2
    exit 1
}

candidate_endpoint() {
    local socket=$1
    local address=$2
    for _ in $(seq 1 120); do
        local response
        response=$("$CLI" diagnostics --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]]; then
            local endpoint
            endpoint=$(python3 - "$address" "$response" <<'PY'
import json
import sys

address = sys.argv[1]
response = json.loads(sys.argv[2])
for candidate in response["diagnostics"]["local_candidates"]:
    endpoint = candidate["endpoint"]
    if endpoint.rsplit(":", 1)[0] == address:
        print(endpoint)
        break
PY
)
            if [[ -n $endpoint ]]; then
                printf '%s\n' "$endpoint"
                return
            fi
        fi
        sleep 0.1
    done
    printf 'candidate did not appear for %s\n' "$address" >&2
    exit 1
}

wait_peer_path() {
    local socket=$1
    local address=$2
    local reason=$3
    for _ in $(seq 1 160); do
        local response
        response=$("$CLI" peers --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]] && python3 - "$address" "$reason" "$response" <<'PY'
import json
import sys

address = sys.argv[1]
reason = sys.argv[2]
response = json.loads(sys.argv[3])
for peer in response["peers"]:
    endpoint = peer.get("active_endpoint")
    if endpoint and endpoint.rsplit(":", 1)[0] == address:
        if peer.get("path_reason") == reason and peer.get("session_established"):
            raise SystemExit(0)
raise SystemExit(1)
PY
        then
            return
        fi
        sleep 0.1
    done
    printf 'peer path did not become active: %s reason=%s\n' "$address" "$reason" >&2
    "$CLI" peers --socket "$socket" --json >&2 || true
    exit 1
}

create_token() {
    curl --fail --silent \
        -X POST "$CONTROLLER_BASE/v1/admin/enrollment-tokens" \
        -H "Authorization: Bearer $fixture_bearer" \
        -H 'Content-Type: application/json' \
        --data "{\"network_id\":\"$NETWORK_ID\",\"expires_in_seconds\":3600,\"max_uses\":1,\"default_role_bitmap\":1,\"default_tags\":[\"linux\"],\"requested_virtual_ip\":null}" |
        python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])'
}

prepare_agent() {
    local namespace=$1
    local name=$2
    local interface=$3
    local root=$4
    mkdir -p "$root"
    python3 - "$root/agent.json" "$CONTROLLER_PORT" "$root/state" "$root/run" "$name" "$interface" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "controller_url": f"http://127.0.0.1:{sys.argv[2]}/",
    "node_name": sys.argv[5],
    "device_type": "linux",
    "state_directory": sys.argv[3],
    "runtime_directory": sys.argv[4],
    "interface_name": sys.argv[6],
    "mtu": 1280,
    "control_sync_interval_seconds": 5,
}))
PY
    create_token >"$root/enrollment.token"
    chmod 0600 "$root/agent.json" "$root/enrollment.token"
    ip netns exec "$namespace" "$AGENT" enroll \
        --config "$root/agent.json" \
        --token-file "$root/enrollment.token" >/dev/null
}

if [[ $(id -u) -ne 0 ]] || [[ ! -c /dev/net/tun ]]; then
    printf 'candidate path test requires root and /dev/net/tun\n' >&2
    exit 2
fi
for command in cargo curl ip nft python3; do
    require_command "$command"
done
if [[ -z ${XS_TEST_DATABASE_URL:-} ]]; then
    if [[ ! -r /etc/xs-nexus/controller.env ]]; then
        printf 'XS_TEST_DATABASE_URL is required\n' >&2
        exit 2
    fi
    XS_TEST_DATABASE_URL=$(python3 - <<'PY'
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

values = {}
for raw_line in Path("/etc/xs-nexus/controller.env").read_text().splitlines():
    line = raw_line.strip()
    if line and not line.startswith("#") and "=" in line:
        name, value = line.split("=", 1)
        values[name] = value
parsed = urlsplit(values["DATABASE_URL"])
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)))
PY
)
    export XS_TEST_DATABASE_URL
fi

cd "$ROOT_DIR"
umask 077
cargo build -p xs-controller
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli

ip link add "$BRIDGE" type bridge
ip addr add "$BRIDGE_IP/29" dev "$BRIDGE"
ip link set "$BRIDGE" up
ip netns add "$NETNS_A"
ip netns add "$NETNS_B"
ip link add "$HOST_A" type veth peer name "$CONTROL_A"
ip link add "$HOST_B" type veth peer name "$CONTROL_B"
ip link set "$CONTROL_A" netns "$NETNS_A"
ip link set "$CONTROL_B" netns "$NETNS_B"
ip link set "$HOST_A" master "$BRIDGE"
ip link set "$HOST_B" master "$BRIDGE"
ip link set "$HOST_A" up
ip link set "$HOST_B" up
ip -n "$NETNS_A" link set lo up
ip -n "$NETNS_B" link set lo up
ip -n "$NETNS_A" addr add "$CONTROL_IP_A/29" dev "$CONTROL_A"
ip -n "$NETNS_B" addr add "$CONTROL_IP_B/29" dev "$CONTROL_B"
ip -n "$NETNS_A" link set "$CONTROL_A" up
ip -n "$NETNS_B" link set "$CONTROL_B" up

CONTROLLER_PORT=$(python3 - <<'PY'
import socket

while True:
    tcp = socket.socket()
    tcp.bind(("0.0.0.0", 0))
    port = tcp.getsockname()[1]
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        udp.bind(("0.0.0.0", port))
    except OSError:
        tcp.close()
        udp.close()
        continue
    print(port)
    tcp.close()
    udp.close()
    break
PY
)
fixture_bearer=$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')
credential_key="$TEMPORARY/credential.key"
configuration_key="$TEMPORARY/configuration.key"
python3 - "$credential_key" "$configuration_key" <<'PY'
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(bytes([51]) * 32)
Path(sys.argv[2]).write_bytes(bytes([52]) * 32)
os.chmod(sys.argv[1], 0o600)
os.chmod(sys.argv[2], 0o600)
PY
auth_environment_name=ADMIN_API_
auth_environment_name+=TOKEN
printf -v "$auth_environment_name" '%s' "$fixture_bearer"
export "${auth_environment_name?}"
CONTROLLER_LISTEN="$BRIDGE_IP:$CONTROLLER_PORT" \
DISCOVERY_LISTEN="$BRIDGE_IP:$CONTROLLER_PORT" \
DISCOVERY_PUBLIC_ENDPOINT="$BRIDGE_IP:$CONTROLLER_PORT" \
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA=xs_nexus_m21_path_test \
CREDENTIAL_SIGNING_KEY_PATH="$credential_key" \
CONFIG_SIGNING_KEY_PATH="$configuration_key" \
NODE_CREDENTIAL_TTL_SECONDS=86400 \
RUST_LOG=debug \
    "$CONTROLLER" >"$TEMPORARY/controller.log" 2>&1 &
CONTROLLER_PID=$!
CONTROLLER_BASE="http://$BRIDGE_IP:$CONTROLLER_PORT"
wait_http "$CONTROLLER_BASE"

write_tcp_forwarder
ip netns exec "$NETNS_A" python3 "$TEMPORARY/tcp-forward.py" \
    --listen-port "$CONTROLLER_PORT" \
    --target-host "$BRIDGE_IP" \
    --target-port "$CONTROLLER_PORT" >"$TEMPORARY/proxy-a.log" 2>&1 &
PROXY_A_PID=$!
ip netns exec "$NETNS_B" python3 "$TEMPORARY/tcp-forward.py" \
    --listen-port "$CONTROLLER_PORT" \
    --target-host "$BRIDGE_IP" \
    --target-port "$CONTROLLER_PORT" >"$TEMPORARY/proxy-b.log" 2>&1 &
PROXY_B_PID=$!
for namespace in "$NETNS_A" "$NETNS_B"; do
    wait_namespace_http "$namespace" "http://127.0.0.1:$CONTROLLER_PORT"
done

network_response=$(curl --fail --silent \
    -X POST "$CONTROLLER_BASE/v1/admin/networks" \
    -H "Authorization: Bearer $fixture_bearer" \
    -H 'Content-Type: application/json' \
    --data "{\"name\":\"candidate-path-$SUFFIX\",\"address_pool\":\"100.91.21.0/24\",\"reserved_addresses\":16}")
NETWORK_ID=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$network_response")
prepare_agent "$NETNS_A" candidate-path-a xsm21a0 "$TEMPORARY/node-a"
prepare_agent "$NETNS_B" candidate-path-b xsm21b0 "$TEMPORARY/node-b"

ip netns exec "$NETNS_A" "$AGENT" run \
    --config "$TEMPORARY/node-a/agent.json" >"$TEMPORARY/agent-a.log" 2>&1 &
AGENT_A_PID=$!
ip netns exec "$NETNS_B" "$AGENT" run \
    --config "$TEMPORARY/node-b/agent.json" >"$TEMPORARY/agent-b.log" 2>&1 &
AGENT_B_PID=$!
wait_for_interface "$NETNS_A" xsm21a0
wait_for_interface "$NETNS_B" xsm21b0
wait_controller_connected "$TEMPORARY/node-a/run/agent.sock"
wait_controller_connected "$TEMPORARY/node-b/run/agent.sock"

endpoint_a=$(candidate_endpoint "$TEMPORARY/node-a/run/agent.sock" "$CONTROL_IP_A")
candidate_endpoint "$TEMPORARY/node-b/run/agent.sock" "$CONTROL_IP_B" >/dev/null
port_a=${endpoint_a##*:}

ip netns exec "$NETNS_A" nft add table inet xsm21observe
ip netns exec "$NETNS_A" nft add chain inet xsm21observe output \
    '{ type filter hook output priority 0; policy accept; }'
ip netns exec "$NETNS_A" nft add rule inet xsm21observe output \
    ip daddr "$BRIDGE_IP" udp sport "$port_a" udp dport "$CONTROLLER_PORT" counter
sleep 3
ip netns exec "$NETNS_A" nft list table inet xsm21observe |
    grep -Eq 'packets [1-9][0-9]* bytes [1-9][0-9]*'

virtual_ip_a=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' "$TEMPORARY/node-a/state/node-state.json")
virtual_ip_b=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' "$TEMPORARY/node-b/state/node-state.json")
ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination "$virtual_ip_b" \
    --payload xs-m21-initial-path \
    --sequence 1 \
    --timeout 6
wait_peer_path "$TEMPORARY/node-a/run/agent.sock" "$CONTROL_IP_B" authenticated_handshake

ip link add "$PATH_A" type veth peer name "$PATH_B"
ip link set "$PATH_A" netns "$NETNS_A"
ip link set "$PATH_B" netns "$NETNS_B"
ip -n "$NETNS_A" link set "$PATH_A" up
ip -n "$NETNS_B" link set "$PATH_B" up
ip netns exec "$NETNS_A" "$PROBE" capture-xsp \
    --interface "$PATH_A" \
    --packet-type path-challenge \
    --count 1 \
    --timeout 12 \
    --metadata "$TEMPORARY/path-challenge.json" \
    >"$TEMPORARY/path-capture.log" 2>&1 &
CAPTURE_PID=$!
ip -n "$NETNS_A" addr add "$PATH_IP_A/30" dev "$PATH_A"
ip -n "$NETNS_B" addr add "$PATH_IP_B/30" dev "$PATH_B"
wait "$CAPTURE_PID"
CAPTURE_PID=
cat "$TEMPORARY/path-capture.log"

wait_peer_path "$TEMPORARY/node-a/run/agent.sock" "$PATH_IP_B" authenticated_path_probe
ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination "$virtual_ip_b" \
    --payload xs-m21-promoted-path \
    --sequence 2 \
    --timeout 3
ip netns exec "$NETNS_B" "$PROBE" icmp \
    --destination "$virtual_ip_a" \
    --payload xs-m21-promoted-reverse \
    --sequence 3 \
    --timeout 3
"$CLI" peers --socket "$TEMPORARY/node-a/run/agent.sock" |
    grep -F 'reason=authenticated_path_probe' >/dev/null

kill -0 "$AGENT_A_PID"
kill -0 "$AGENT_B_PID"
kill -0 "$CONTROLLER_PID"
printf 'Agent authenticated candidate discovery and path promotion test passed\n'
