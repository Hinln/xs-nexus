#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
PROBE="$ROOT_DIR/scripts/xsp-network-probe.py"
CONTROLLER="$ROOT_DIR/target/debug/xs-controller"
RELAY="$ROOT_DIR/target/debug/xs-relay"
AGENT="$ROOT_DIR/target/debug/xs-agent"
CLI="$ROOT_DIR/target/debug/xs"
KEY_DERIVER="$ROOT_DIR/target/debug/examples/derive_ed25519_public"
SCHEMA_RESET="$ROOT_DIR/target/debug/examples/reset_test_schema"
TEMPORARY=$(mktemp -d /tmp/xs-m23-relay.XXXXXX)
TEST_DATABASE_SCHEMA=xs_nexus_m23_relay_test
SUFFIX=$(printf '%04x' "$(( $$ % 65536 ))")
BRIDGE="xm23b$SUFFIX"
NETNS_A="xsm23a-$SUFFIX"
NETNS_B="xsm23b-$SUFFIX"
HOST_A="xm23ha$SUFFIX"
HOST_B="xm23hb$SUFFIX"
CONTROL_A="xm23ca"
CONTROL_B="xm23cb"
BRIDGE_IP=10.250.23.1
CONTROL_IP_A=10.250.23.2
CONTROL_IP_B=10.250.23.3
CONTROLLER_PID=
RELAY_1_PID=
RELAY_2_PID=
AGENT_A_PID=
AGENT_B_PID=
PROXY_A_PID=
PROXY_B_PID=
CAPTURE_PID=
COLLECT_PID=

cleanup() {
    local status=$?
    set +e
    for pid in "$CAPTURE_PID" "$COLLECT_PID" "$AGENT_A_PID" "$AGENT_B_PID" \
        "$PROXY_A_PID" "$PROXY_B_PID" "$RELAY_1_PID" "$RELAY_2_PID" "$CONTROLLER_PID"
    do
        if [[ -n $pid ]]; then
            kill "$pid" >/dev/null 2>&1
            wait "$pid" >/dev/null 2>&1
        fi
    done
    if [[ -x $SCHEMA_RESET && -n ${XS_TEST_DATABASE_URL:-} ]]; then
        DATABASE_URL="$XS_TEST_DATABASE_URL" \
        DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
            "$SCHEMA_RESET" >/dev/null 2>&1
    fi
    for namespace in "$NETNS_A" "$NETNS_B"; do
        if ip netns list | awk '{print $1}' | grep -Fxq "$namespace"; then
            ip netns exec "$namespace" nft delete table inet xsm23block >/dev/null 2>&1
        fi
    done
    ip netns del "$NETNS_A" >/dev/null 2>&1
    ip netns del "$NETNS_B" >/dev/null 2>&1
    ip link del "$BRIDGE" >/dev/null 2>&1
    if (( status != 0 )); then
        for log in controller.log relay-1.log relay-2.log agent-a.log agent-b.log \
            proxy-a.log proxy-b.log relay-capture.log collector.log
        do
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
    local base_url=$1
    local label=$2
    for _ in $(seq 1 120); do
        if curl --fail --silent "$base_url/health/ready" >/dev/null; then
            return
        fi
        sleep 0.1
    done
    printf '%s did not become ready\n' "$label" >&2
    exit 1
}

wait_namespace_http() {
    local namespace=$1
    local base_url=$2
    for _ in $(seq 1 120); do
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
    for _ in $(seq 1 120); do
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
    for _ in $(seq 1 160); do
        local response
        response=$("$CLI" status --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]]; then
            if python3 - "$response" <<'PY'
import json
import sys

raise SystemExit(not json.loads(sys.argv[1])["status"]["controller_connected"])
PY
            then
                return
            fi
        fi
        sleep 0.1
    done
    printf 'Agent control connection did not become ready: %s\n' "$socket" >&2
    exit 1
}

wait_configuration_version() {
    local socket=$1
    local expected=$2
    for _ in $(seq 1 160); do
        local response
        response=$("$CLI" status --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]] &&
            python3 - "$response" "$expected" <<'PY'
import json
import sys

status = json.loads(sys.argv[1])["status"]
raise SystemExit(status["configuration_version"] < int(sys.argv[2]))
PY
        then
            return
        fi
        sleep 0.1
    done
    printf 'Agent did not apply configuration version %s: %s\n' "$expected" "$socket" >&2
    exit 1
}

install_test_acl() {
    local response
    response=$(curl --fail --silent \
        -X PUT "$CONTROLLER_BASE/v1/admin/networks/$NETWORK_ID/acl" \
        -H "Authorization: Bearer $fixture_bearer" \
        -H 'Content-Type: application/json' \
        --data '{
            "expected_policy_version": 1,
            "groups": [],
            "rules": [{
                "id": "allow-test-traffic",
                "priority": 100,
                "action": "allow",
                "sources": [{"type": "any"}],
                "destinations": [{"type": "any"}],
                "protocol": "any",
                "destination_ports": []
            }]
        }')
    ACL_CONFIGURATION_VERSION=$(python3 -c \
        'import json,sys; print(json.load(sys.stdin)["configuration_version"])' \
        <<<"$response")
}

candidate_endpoint() {
    local socket=$1
    local address=$2
    for _ in $(seq 1 160); do
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
    local endpoint=$2
    local reason=$3
    for _ in $(seq 1 320); do
        local response
        response=$("$CLI" peers --socket "$socket" --json 2>/dev/null || true)
        if [[ -n $response ]] && python3 - "$endpoint" "$reason" "$response" <<'PY'
import json
import sys

endpoint = sys.argv[1]
reason = sys.argv[2]
response = json.loads(sys.argv[3])
for peer in response["peers"]:
    if (
        peer.get("active_endpoint") == endpoint
        and peer.get("path_reason") == reason
        and peer.get("session_established")
    ):
        raise SystemExit(0)
raise SystemExit(1)
PY
        then
            return
        fi
        sleep 0.1
    done
    printf 'peer path did not become active: %s reason=%s\n' "$endpoint" "$reason" >&2
    "$CLI" peers --socket "$socket" --json >&2 || true
    exit 1
}

relay_metric() {
    local base_url=$1
    local field=$2
    curl --fail --silent "$base_url/metrics" |
        python3 -c "import json,sys; print(json.load(sys.stdin)['$field'])"
}

wait_relay_metric() {
    local base_url=$1
    local field=$2
    local minimum=$3
    for _ in $(seq 1 160); do
        local value
        value=$(relay_metric "$base_url" "$field" 2>/dev/null || printf '0')
        if (( value >= minimum )); then
            return
        fi
        sleep 0.1
    done
    printf 'Relay metric did not reach target: %s minimum=%s\n' "$field" "$minimum" >&2
    exit 1
}

wait_relay_metric_kreater() {
    local base_url=$1
    local field=$2
    local previous=$3
    for _ in $(seq 1 80); do
        local value
        value=$(relay_metric "$base_url" "$field" 2>/dev/null || printf '0')
        if (( value > previous )); then
            return
        fi
        sleep 0.1
    done
    printf 'Relay metric did not increase: %s previous=%s\n' "$field" "$previous" >&2
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

block_direct() {
    ip netns exec "$NETNS_A" nft add table inet xsm23block
    ip netns exec "$NETNS_A" nft add chain inet xsm23block output \
        '{ type filter hook output priority 0; policy accept; }'
    ip netns exec "$NETNS_A" nft add rule inet xsm23block output \
        ip daddr "$CONTROL_IP_B" meta l4proto udp drop
    ip netns exec "$NETNS_B" nft add table inet xsm23block
    ip netns exec "$NETNS_B" nft add chain inet xsm23block output \
        '{ type filter hook output priority 0; policy accept; }'
    ip netns exec "$NETNS_B" nft add rule inet xsm23block output \
        ip daddr "$CONTROL_IP_A" meta l4proto udp drop
}

unblock_direct() {
    ip netns exec "$NETNS_A" nft delete table inet xsm23block
    ip netns exec "$NETNS_B" nft delete table inet xsm23block
}

if [[ $(id -u) -ne 0 ]] || [[ ! -c /dev/net/tun ]]; then
    printf 'Relay path test requires root and /dev/net/tun\n' >&2
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
cargo build -p xs-controller --example reset_test_schema
cargo build -p xs-relay
cargo build -p xs-agent --features privileged-network-tests --bin xs-agent
cargo build -p xs-cli
cargo build -p xs-protocol --example derive_ed25519_public
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
    "$SCHEMA_RESET"

mapfile -t ports < <(python3 - <<'PY'
import socket


def dual_port():
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
        tcp.close()
        udp.close()
        return port


def port(kind):
    sock_type = socket.SOCK_DGRAM if kind == "udp" else socket.SOCK_STREAM
    with socket.socket(socket.AF_INET, sock_type) as sock:
        sock.bind(("0.0.0.0", 0))
        return sock.getsockname()[1]


selected = [dual_port()]
for kind in ("udp", "udp", "tcp", "tcp"):
    while True:
        candidate = port(kind)
        if candidate not in selected:
            selected.append(candidate)
            break
for value in selected:
    print(value)
PY
)
CONTROLLER_PORT=${ports[0]}
RELAY_PORT_1=${ports[1]}
RELAY_PORT_2=${ports[2]}
RELAY_HEALTH_PORT_1=${ports[3]}
RELAY_HEALTH_PORT_2=${ports[4]}

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
block_direct

credential_key="$TEMPORARY/credential.key"
configuration_key="$TEMPORARY/configuration.key"
relay_key_1="$TEMPORARY/relay-1.key"
relay_key_2="$TEMPORARY/relay-2.key"
credential_public="$TEMPORARY/credential.pub"
relay_public_1="$TEMPORARY/relay-1.pub"
relay_public_2="$TEMPORARY/relay-2.pub"
python3 - "$credential_key" "$configuration_key" "$relay_key_1" "$relay_key_2" <<'PY'
import os
import sys
from pathlib import Path

for path, seed in zip(sys.argv[1:], (71, 72, 73, 74), strict=True):
    Path(path).write_bytes(bytes([seed]) * 32)
    os.chmod(path, 0o600)
PY
"$KEY_DERIVER" "$credential_key" "$credential_public"
"$KEY_DERIVER" "$relay_key_1" "$relay_public_1"
"$KEY_DERIVER" "$relay_key_2" "$relay_public_2"
chmod 0600 "$credential_public" "$relay_public_1" "$relay_public_2"

RELAY_ID_1=$(python3 -c 'import base64; print(base64.urlsafe_b64encode(bytes([81])*16).rstrip(b"=").decode())')
RELAY_ID_2=$(python3 -c 'import base64; print(base64.urlsafe_b64encode(bytes([82])*16).rstrip(b"=").decode())')
relay_catalog="$TEMPORARY/relays.json"
python3 - "$relay_catalog" "$RELAY_ID_1" "$BRIDGE_IP:$RELAY_PORT_1" "$relay_public_1" \
    "$RELAY_ID_2" "$BRIDGE_IP:$RELAY_PORT_2" "$relay_public_2" <<'PY'
import base64
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

expires = (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat().replace("+00:00", "Z")
relays = []
for offset in (2, 5):
    relays.append({
        "relay_id_base64": sys.argv[offset],
        "endpoint": sys.argv[offset + 1],
        "identity_public_key_base64": base64.urlsafe_b64encode(
            Path(sys.argv[offset + 2]).read_bytes()
        ).rstrip(b"=").decode(),
        "priority": 300 if offset == 2 else 200,
        "expires_at": expires,
    })
Path(sys.argv[1]).write_text(json.dumps(relays))
PY
chmod 0600 "$relay_catalog"

RELAY_LISTEN="$BRIDGE_IP:$RELAY_PORT_1" \
RELAY_HEALTH_LISTEN="127.0.0.1:$RELAY_HEALTH_PORT_1" \
RELAY_ID_BASE64="$RELAY_ID_1" \
CONTROLLER_CREDENTIAL_PUBLIC_KEY_PATH="$credential_public" \
RELAY_IDENTITY_KEY_PATH="$relay_key_1" \
RELAY_LEASE_TTL_SECONDS=30 \
RELAY_IDLE_TIMEOUT_SECONDS=15 \
RELAY_MAX_LEASES=16 \
RELAY_REGISTRATIONS_PER_SOURCE_PER_MINUTE=60 \
RELAY_PACKETS_PER_LEASE_PER_SECOND=1000 \
RELAY_BYTES_PER_LEASE_PER_SECOND=1048576 \
RELAY_QUEUE_PACKETS_PER_NODE=32 \
RELAY_QUEUE_BYTES_PER_NODE=65536 \
RUST_LOG=debug \
    "$RELAY" >"$TEMPORARY/relay-1.log" 2>&1 &
RELAY_1_PID=$!
RELAY_LISTEN="$BRIDGE_IP:$RELAY_PORT_2" \
RELAY_HEALTH_LISTEN="127.0.0.1:$RELAY_HEALTH_PORT_2" \
RELAY_ID_BASE64="$RELAY_ID_2" \
CONTROLLER_CREDENTIAL_PUBLIC_KEY_PATH="$credential_public" \
RELAY_IDENTITY_KEY_PATH="$relay_key_2" \
RELAY_LEASE_TTL_SECONDS=30 \
RELAY_IDLE_TIMEOUT_SECONDS=15 \
RELAY_MAX_LEASES=16 \
RELAY_REGISTRATIONS_PER_SOURCE_PER_MINUTE=60 \
RELAY_PACKETS_PER_LEASE_PER_SECOND=1000 \
RELAY_BYTES_PER_LEASE_PER_SECOND=1048576 \
RELAY_QUEUE_PACKETS_PER_NODE=32 \
RELAY_QUEUE_BYTES_PER_NODE=65536 \
RUST_LOG=debug \
    "$RELAY" >"$TEMPORARY/relay-2.log" 2>&1 &
RELAY_2_PID=$!
RELAY_HEALTH_1="http://127.0.0.1:$RELAY_HEALTH_PORT_1"
RELAY_HEALTH_2="http://127.0.0.1:$RELAY_HEALTH_PORT_2"
wait_http "$RELAY_HEALTH_1" "Relay 1"
wait_http "$RELAY_HEALTH_2" "Relay 2"

fixture_bearer=$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')
auth_environment_name=ADMIN_API_
auth_environment_name+=TOKEN
printf -v "$auth_environment_name" '%s' "$fixture_bearer"
export "${auth_environment_name?}"
CONTROLLER_LISTEN="$BRIDGE_IP:$CONTROLLER_PORT" \
DISCOVERY_LISTEN="$BRIDGE_IP:$CONTROLLER_PORT" \
DISCOVERY_PUBLIC_ENDPOINT="$BRIDGE_IP:$CONTROLLER_PORT" \
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
CREDENTIAL_SIGNING_KEY_PATH="$credential_key" \
CONFIG_SIGNING_KEY_PATH="$configuration_key" \
NODE_CREDENTIAL_TTL_SECONDS=86400 \
RELAY_CATALOG_PATH="$relay_catalog" \
RUST_LOG=debug \
    "$CONTROLLER" >"$TEMPORARY/controller.log" 2>&1 &
CONTROLLER_PID=$!
CONTROLLER_BASE="http://$BRIDGE_IP:$CONTROLLER_PORT"
wait_http "$CONTROLLER_BASE" "Controller"

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
    --data "{\"name\":\"relay-path-$SUFFIX\",\"address_pool\":\"100.93.23.0/24\",\"reserved_addresses\":16}")
NETWORK_ID=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$network_response")
install_test_acl
prepare_agent "$NETNS_A" relay-path-a xsm23a0 "$TEMPORARY/node-a"
prepare_agent "$NETNS_B" relay-path-b xsm23b0 "$TEMPORARY/node-b"

ip netns exec "$NETNS_A" "$AGENT" run \
    --config "$TEMPORARY/node-a/agent.json" >"$TEMPORARY/agent-a.log" 2>&1 &
AGENT_A_PID=$!
ip netns exec "$NETNS_B" "$AGENT" run \
    --config "$TEMPORARY/node-b/agent.json" >"$TEMPORARY/agent-b.log" 2>&1 &
AGENT_B_PID=$!
wait_for_interface "$NETNS_A" xsm23a0
wait_for_interface "$NETNS_B" xsm23b0
wait_controller_connected "$TEMPORARY/node-a/run/agent.sock"
wait_controller_connected "$TEMPORARY/node-b/run/agent.sock"
wait_configuration_version "$TEMPORARY/node-a/run/agent.sock" "$ACL_CONFIGURATION_VERSION"
wait_configuration_version "$TEMPORARY/node-b/run/agent.sock" "$ACL_CONFIGURATION_VERSION"

candidate_endpoint "$TEMPORARY/node-a/run/agent.sock" "$CONTROL_IP_A" >/dev/null
endpoint_b=$(candidate_endpoint "$TEMPORARY/node-b/run/agent.sock" "$CONTROL_IP_B")
wait_relay_metric "$RELAY_HEALTH_1" active_leases 2
wait_relay_metric "$RELAY_HEALTH_2" active_leases 2
wait_peer_path "$TEMPORARY/node-a/run/agent.sock" "$BRIDGE_IP:$RELAY_PORT_1" relay_fallback

virtual_ip_a=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' "$TEMPORARY/node-a/state/node-state.json")
virtual_ip_b=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' "$TEMPORARY/node-b/state/node-state.json")
marker=xs-m23-relay-ciphertext
ip netns exec "$NETNS_B" "$PROBE" collect-udp \
    --bind "$virtual_ip_b" \
    --port 43231 \
    --expected "$marker" \
    --count 1 \
    --timeout 8 >"$TEMPORARY/collector.log" 2>&1 &
COLLECT_PID=$!
"$PROBE" capture-relay \
    --interface "$BRIDGE" \
    --destination "$BRIDGE_IP" \
    --destination-port "$RELAY_PORT_1" \
    --inner-packet-type data \
    --count 1 \
    --timeout 8 \
    --forbid-text "$marker" \
    --forbid-file "$TEMPORARY/original-packet.bin" \
    --output "$TEMPORARY/relay-frame.bin" \
    --metadata "$TEMPORARY/relay-frame.json" \
    >"$TEMPORARY/relay-capture.log" 2>&1 &
CAPTURE_PID=$!
sleep 0.2
ip netns exec "$NETNS_A" "$PROBE" send-virtual \
    --source "$virtual_ip_a" \
    --destination "$virtual_ip_b" \
    --source-port 43230 \
    --destination-port 43231 \
    --payload "$marker" \
    --packet-id 23 \
    --output "$TEMPORARY/original-packet.bin"
wait "$COLLECT_PID"
COLLECT_PID=
wait "$CAPTURE_PID"
CAPTURE_PID=
cat "$TEMPORARY/collector.log"
cat "$TEMPORARY/relay-capture.log"
wait_relay_metric "$RELAY_HEALTH_1" packets_forwarded 1
wait_relay_metric "$RELAY_HEALTH_1" packets_received 1
wait_relay_metric "$RELAY_HEALTH_1" bytes_received 1
wait_relay_metric "$RELAY_HEALTH_1" bytes_forwarded 1
wait_relay_metric "$RELAY_HEALTH_1" forwarding_latency_samples 1

authentication_before=$(relay_metric "$RELAY_HEALTH_1" authentication_drops)
python3 - "$TEMPORARY/relay-frame.bin" "$BRIDGE_IP" "$RELAY_PORT_1" <<'PY'
import socket
import sys
from pathlib import Path

with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
    sock.sendto(Path(sys.argv[1]).read_bytes(), (sys.argv[2], int(sys.argv[3])))
PY
wait_relay_metric_kreater "$RELAY_HEALTH_1" authentication_drops "$authentication_before"
wait_relay_metric "$RELAY_HEALTH_1" packets_dropped 1

kill "$RELAY_1_PID"
wait "$RELAY_1_PID"
RELAY_1_PID=
wait_peer_path "$TEMPORARY/node-a/run/agent.sock" "$BRIDGE_IP:$RELAY_PORT_2" relay_failover
ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination "$virtual_ip_b" \
    --payload xs-m23-relay-failover \
    --sequence 24 \
    --timeout 8
wait_relay_metric "$RELAY_HEALTH_2" packets_forwarded 1

unblock_direct
wait_peer_path "$TEMPORARY/node-a/run/agent.sock" "$endpoint_b" authenticated_path_probe
ip netns exec "$NETNS_A" "$PROBE" icmp \
    --destination "$virtual_ip_b" \
    --payload xs-m23-direct-restored \
    --sequence 25 \
    --timeout 5
ip netns exec "$NETNS_B" "$PROBE" icmp \
    --destination "$virtual_ip_a" \
    --payload xs-m23-direct-reverse \
    --sequence 26 \
    --timeout 5
"$CLI" peers --socket "$TEMPORARY/node-a/run/agent.sock" |
    grep -F "active=$endpoint_b" |
    grep -F 'reason=authenticated_path_probe' >/dev/null

kill -0 "$AGENT_A_PID"
kill -0 "$AGENT_B_PID"
kill -0 "$RELAY_2_PID"
kill -0 "$CONTROLLER_PID"
printf 'Agent Relay fallback, ciphertext, failover, and Direct restoration test passed\n'
