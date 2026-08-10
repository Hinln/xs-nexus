#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TEST_INTERFACE=xssvc0

if [[ $(id -u) -ne 0 ]]; then
    printf 'Agent systemd lifecycle test requires root\n' >&2
    exit 2
fi
for command in cargo curl python3 systemctl systemd-analyze systemd-run nsenter ip; do
    if ! command -v "$command" >/dev/null; then
        printf 'required command is unavailable: %s\n' "$command" >&2
        exit 2
    fi
done
if [[ ! -c /dev/net/tun ]]; then
    printf '/dev/net/tun is unavailable\n' >&2
    exit 2
fi
if [[ -e "/sys/class/net/$TEST_INTERFACE" ]]; then
    printf 'refusing to run while host interface %s exists\n' "$TEST_INTERFACE" >&2
    exit 2
fi

if [[ -z "${XS_TEST_DATABASE_URL:-}" ]]; then
    if [[ ! -r /etc/xs-nexus/controller.env ]]; then
        printf 'XS_TEST_DATABASE_URL is required when /etc/xs-nexus/controller.env is unavailable\n' >&2
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

temporary=$(mktemp -d)
unit="xs-agent-m12-$RANDOM-$$"
controller_pid=
install_directory_created=false
current_step=initialization
schema="xs_nexus_agent_systemd_test"
test_install_root="/usr/local/lib/xs-nexus-tests/$unit"
test_agent_binary="$test_install_root/bin/xs-agent"

report_failure() {
    status=$?
    printf 'Agent systemd test failed during step: %s\n' "$current_step" >&2
    if [[ -f "$temporary/controller.log" ]]; then
        tail -n 50 "$temporary/controller.log" >&2
    fi
    if systemctl cat "$unit.service" >/dev/null 2>&1; then
        journalctl -u "$unit.service" --no-pager -n 50 >&2
    fi
    return "$status"
}

cleanup() {
    set +e
    systemctl stop "$unit.service" >/dev/null 2>&1
    systemctl reset-failed "$unit.service" >/dev/null 2>&1
    if [[ -n "$controller_pid" ]]; then
        kill "$controller_pid" >/dev/null 2>&1
        wait "$controller_pid" >/dev/null 2>&1
    fi
    if [[ -x "$ROOT_DIR/target/debug/examples/reset_test_schema" ]]; then
        DATABASE_URL="$XS_TEST_DATABASE_URL" \
        DATABASE_SCHEMA="$schema" \
            "$ROOT_DIR/target/debug/examples/reset_test_schema" \
            >/dev/null 2>&1
    fi
    if [[ "$install_directory_created" == true ]]; then
        rm -f "$test_agent_binary"
        rmdir "$test_install_root/bin" >/dev/null 2>&1
        rmdir "$test_install_root" >/dev/null 2>&1
        rmdir /usr/local/lib/xs-nexus-tests >/dev/null 2>&1
    fi
    rm -rf "$temporary"
    if [[ -e "/sys/class/net/$TEST_INTERFACE" ]]; then
        printf 'Agent systemd test leaked %s into the host namespace\n' "$TEST_INTERFACE" >&2
        exit 1
    fi
}
trap report_failure ERR
trap cleanup EXIT INT TERM

umask 077
credential_key="$temporary/credential.key"
configuration_key="$temporary/configuration.key"
python3 - "$credential_key" "$configuration_key" <<'PY'
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(bytes([41]) * 32)
Path(sys.argv[2]).write_bytes(bytes([42]) * 32)
os.chmod(sys.argv[1], 0o600)
os.chmod(sys.argv[2], 0o600)
PY

port=$(python3 - <<'PY'
import socket

with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
)
controller_url="http://127.0.0.1:$port/"
controller_base=${controller_url%/}
test_auth_value="agent-systemd-integration-auth-value-32"

cd "$ROOT_DIR"
current_step=build
cargo build -p xs-controller -p xs-agent -p xs-cli
cargo build -p xs-controller --example reset_test_schema
current_step=database_reset
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA="$schema" \
    "$ROOT_DIR/target/debug/examples/reset_test_schema"
database_expected_role=$(python3 -c 'import sys; from urllib.parse import urlsplit; print(urlsplit(sys.stdin.read()).username or "")' <<<"$XS_TEST_DATABASE_URL")
[[ $database_expected_role =~ ^[a-z_][a-z0-9_]{0,62}$ ]]
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA="$schema" \
    "$ROOT_DIR/target/debug/xs-controller" migrate
if [[ -e "$test_install_root" ]]; then
    printf 'refusing to reuse test install path: %s\n' "$test_install_root" >&2
    exit 2
fi
mkdir -p "$test_install_root/bin"
install_directory_created=true
install -m 0755 "$ROOT_DIR/target/debug/xs-agent" "$test_agent_binary"
current_step=controller_start
auth_environment_name="ADMIN_API_"
auth_environment_name+="TOKEN"
printf -v "$auth_environment_name" '%s' "$test_auth_value"
export "${auth_environment_name?}"
CONTROLLER_LISTEN="127.0.0.1:$port" \
DATABASE_URL="$XS_TEST_DATABASE_URL" \
DATABASE_SCHEMA="$schema" \
DATABASE_EXPECTED_ROLE="$database_expected_role" \
CREDENTIAL_SIGNING_KEY_PATH="$credential_key" \
CONFIG_SIGNING_KEY_PATH="$configuration_key" \
NODE_CREDENTIAL_TTL_SECONDS=86400 \
RUST_LOG=warn \
    "$ROOT_DIR/target/debug/xs-controller" >"$temporary/controller.log" 2>&1 &
controller_pid=$!

current_step=controller_ready
for _ in {1..100}; do
    if curl --fail --silent "$controller_base/health/ready" >/dev/null; then
        break
    fi
    if ! kill -0 "$controller_pid" 2>/dev/null; then
        cat "$temporary/controller.log" >&2
        exit 1
    fi
    sleep 0.1
done
curl --fail --silent "$controller_base/health/ready" >/dev/null

current_step=create_network
network_response=$(curl --fail --silent \
    -X POST "$controller_base/v1/admin/networks" \
    -H "Authorization: Bearer $test_auth_value" \
    -H 'Content-Type: application/json' \
    --data "{\"name\":\"agent-systemd-$RANDOM-$$\",\"address_pool\":\"100.89.20.0/24\",\"reserved_addresses\":16}")
network_id=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$network_response")
current_step=create_token
token_response=$(curl --fail --silent \
    -X POST "$controller_base/v1/admin/enrollment-tokens" \
    -H "Authorization: Bearer $test_auth_value" \
    -H 'Content-Type: application/json' \
    --data "{\"network_id\":\"$network_id\",\"expires_in_seconds\":3600,\"max_uses\":1,\"default_role_bitmap\":1,\"default_tags\":[\"linux\"],\"requested_virtual_ip\":null}")
enrollment_value=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])' <<<"$token_response")

state_directory="$temporary/state"
runtime_directory="$temporary/run"
config_path="$temporary/agent.json"
token_path="$temporary/enrollment.token"
python3 - "$config_path" "$controller_url" "$state_directory" "$runtime_directory" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "controller_url": sys.argv[2],
    "node_name": "agent-systemd-node",
    "device_type": "linux",
    "state_directory": sys.argv[3],
    "runtime_directory": sys.argv[4],
    "interface_name": "xssvc0",
    "mtu": 1280,
    "control_sync_interval_seconds": 5,
}))
PY
printf '%s' "$enrollment_value" >"$token_path"
chmod 0600 "$config_path" "$token_path"
current_step=enroll
"$ROOT_DIR/target/debug/xs-agent" enroll --config "$config_path" --token-file "$token_path" >/dev/null
test ! -e "$token_path"
mkdir -p "$runtime_directory"
chmod 0700 "$runtime_directory"

current_step=controller_stop
kill "$controller_pid"
wait "$controller_pid"
controller_pid=

current_step=unit_verify
systemd-analyze verify "$ROOT_DIR/deploy/systemd/xs-agent.service"
current_step=unit_start
systemd-run \
    --unit="$unit" \
    --collect \
    --quiet \
    --property=Type=simple \
    --property=Restart=on-failure \
    --property=RestartSec=1s \
    --property=PrivateNetwork=yes \
    --property=CapabilityBoundingSet=CAP_NET_ADMIN \
    --property=AmbientCapabilities=CAP_NET_ADMIN \
    --property=NoNewPrivileges=yes \
    --property=DevicePolicy=closed \
    --property='DeviceAllow=/dev/net/tun rw' \
    --property=PrivateDevices=no \
    --property=ProtectSystem=strict \
    --property=ProtectHome=yes \
    --property="ReadWritePaths=$state_directory $runtime_directory" \
    --property='RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK' \
    --property=RuntimeMaxSec=120s \
    "$test_agent_binary" run --config "$config_path"

current_step=socket_wait
socket_path="$runtime_directory/agent.sock"
for _ in {1..100}; do
    if [[ -S "$socket_path" ]]; then
        break
    fi
    if ! systemctl is-active --quiet "$unit.service"; then
        journalctl -u "$unit.service" --no-pager -n 50 >&2
        exit 1
    fi
    sleep 0.1
done
test -S "$socket_path"

current_step=namespace_inspection
main_pid=$(systemctl show --property=MainPID --value "$unit.service")
nsenter --target "$main_pid" --net ip link show dev "$TEST_INTERFACE" >/dev/null
test ! -e "/sys/class/net/$TEST_INTERFACE"

current_step=cli_diagnostics
status_output=$("$ROOT_DIR/target/debug/xs" status --socket "$socket_path")
grep -F 'network=active' <<<"$status_output" >/dev/null
grep -F 'interface=xssvc0' <<<"$status_output" >/dev/null
peers_output=$("$ROOT_DIR/target/debug/xs" peers --socket "$socket_path")
grep -F 'configured_peers=0 returned=0 truncated=false' <<<"$peers_output" >/dev/null
diagnostics_output=$("$ROOT_DIR/target/debug/xs" diagnostics --socket "$socket_path")
grep -F 'address_pool=100.89.20.0/24' <<<"$diagnostics_output" >/dev/null

current_step=crash_restart
test -f "$state_directory/network-manifest.json"
initial_main_pid=$main_pid
systemctl kill --kill-whom=main --signal=SIGKILL "$unit.service"
restarted_main_pid=0
for _ in {1..150}; do
    candidate_pid=$(systemctl show --property=MainPID --value "$unit.service")
    if systemctl is-active --quiet "$unit.service" \
        && [[ $candidate_pid =~ ^[1-9][0-9]*$ ]] \
        && [[ $candidate_pid != "$initial_main_pid" ]] \
        && "$ROOT_DIR/target/debug/xs" status --socket "$socket_path" >/dev/null 2>&1; then
        restarted_main_pid=$candidate_pid
        break
    fi
    sleep 0.1
done
[[ $restarted_main_pid =~ ^[1-9][0-9]*$ ]]
restart_count=$(systemctl show --property=NRestarts --value "$unit.service")
[[ $restart_count =~ ^[0-9]+$ ]]
(( restart_count == 1 ))
test -f "$state_directory/network-manifest.json"
nsenter --target "$restarted_main_pid" --net ip link show dev "$TEST_INTERFACE" >/dev/null
test ! -e "/sys/class/net/$TEST_INTERFACE"

current_step=network_change
nsenter --target "$restarted_main_pid" --net ip link set dev lo down
sleep 0.5
systemctl is-active --quiet "$unit.service"
[[ $(systemctl show --property=MainPID --value "$unit.service") == "$restarted_main_pid" ]]
nsenter --target "$restarted_main_pid" --net ip link set dev lo up
"$ROOT_DIR/target/debug/xs" status --socket "$socket_path" >/dev/null

current_step=unit_stop
systemctl stop "$unit.service"
for _ in {1..50}; do
    ! systemctl is-active --quiet "$unit.service" && break
    sleep 0.1
done
if systemctl is-active --quiet "$unit.service"; then
    exit 1
fi

current_step=crash_cleanup
"$ROOT_DIR/target/debug/xs-agent" cleanup --config "$config_path" >/dev/null
test ! -e "$state_directory/network-manifest.json"
rm -f "$socket_path"
test ! -e "/sys/class/net/$TEST_INTERFACE"
printf 'initial_main_pid=%s\n' "$initial_main_pid"
printf 'restarted_main_pid=%s\n' "$restarted_main_pid"
printf 'restart_count=%s\n' "$restart_count"
printf 'network_change=PASS\n'
printf 'crash_cleanup=PASS\n'
printf 'status=PASS\n'
