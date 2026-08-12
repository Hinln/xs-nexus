#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"
SOAK_COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.soak.yaml"
SUMMARY_SCRIPT="$ROOT_DIR/scripts/summarize-current-revision-soak.py"
DURATION_SECONDS=${XS_SOAK_DURATION_SECONDS:-86400}
SAMPLE_INTERVAL_SECONDS=${XS_SOAK_SAMPLE_INTERVAL_SECONDS:-60}
CALIBRATION=${XS_SOAK_CALIBRATION:-0}
EVIDENCE_DIR=${EVIDENCE_DIR:-"$ROOT_DIR/artifacts/qa/current-revision-soak-$(date -u +%Y%m%dT%H%M%SZ)"}
AGENT_BUILDER_IMAGE=${XS_SOAK_AGENT_BUILDER_IMAGE:-rust:1.94.0-bookworm@sha256:365468470075493dc4583f47387001854321c5a8583ea9604b297e67f01c5a4f}
AGENT_RUNTIME_IMAGE=${XS_SOAK_AGENT_RUNTIME_IMAGE:-gcr.io/distroless/cc-debian12:nonroot@sha256:fccdbb0a547c14e23fcf4ce8ad62ca5d43b4faae8d22cd292f490fef9946c96e}
ALPINE_IMAGE=${XS_SOAK_ALPINE_IMAGE:-alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce}
POSTGRES_IMAGE=postgres:18-alpine3.22@sha256:774521500f4c22761b25a6bdb772a0a3c2e8dd32468210bdad9231c5752ea398
SUFFIX=$(openssl rand -hex 4)
PROJECT="xs-nexus-gate22-$SUFFIX"
DEPLOYMENT="gate22-$SUFFIX"
NETWORK="xs-gate22-$SUFFIX"
LAN_NETWORK="xs-gate22-lan-$SUFFIX"
PRIME_NETWORK="xs-gate22-prime-$SUFFIX"
AGENT_A="xs-gate22-agent-a-$SUFFIX"
AGENT_B="xs-gate22-agent-b-$SUFFIX"
AGENT_C="xs-gate22-agent-c-$SUFFIX"
AGENT_A_NETNS="xs-gate22-agent-a-netns-$SUFFIX"
AGENT_B_NETNS="xs-gate22-agent-b-netns-$SUFFIX"
AGENT_C_NETNS="xs-gate22-agent-c-netns-$SUFFIX"
LAN_TARGET="xs-gate22-lan-target-$SUFFIX"
TEMPORARY=$(mktemp -d /tmp/xs-gate22.XXXXXX)
ENVIRONMENT_FILE="$TEMPORARY/gate22.compose.env"
CONTROLLER_SECRETS="$TEMPORARY/controller"
DATABASE_SECRETS="$TEMPORARY/database"
POSTGRES_SECRETS="$TEMPORARY/postgres"
RELAY_SECRETS="$TEMPORARY/relay"
RELEASE_DIRECTORY="$TEMPORARY/releases/linux/stable"
WINDOWS_RELEASE_DIRECTORY="$TEMPORARY/releases/windows/stable"
BACKUP_DIRECTORY="$TEMPORARY/backups"
REPLICA_DIRECTORY="$TEMPORARY/replica"
STATE_DIRECTORY="$TEMPORARY/state"
BINARY_DIRECTORY="$TEMPORARY/bin"
AGENT_BUILD_TARGET_DIRECTORY="$TEMPORARY/agent-target"
AGENT_PROXY_SCRIPT="$TEMPORARY/controller-loopback-proxy.sh"
AGENT_A_ROOT="$TEMPORARY/agent-a"
AGENT_B_ROOT="$TEMPORARY/agent-b"
AGENT_C_ROOT="$TEMPORARY/agent-c"
DATABASE_SCHEMA=gate22_schema
DATABASE_OWNER_ROLE=gate22_owner
DATABASE_APP_ROLE=gate22_app
DATABASE_MIGRATOR_ROLE=gate22_migrator
CONTROLLER_PORT=
CONSOLE_PORT=
DISCOVERY_PORT=
RELAY_PORT=
UNDERLAY_SUBNET=
LAN_SUBNET=
CONTROLLER_IP=
RELAY_IP=
POSTGRES_IP=
CONSOLE_IP=
AGENT_A_IP=
AGENT_B_IP=
AGENT_C_IP=
LAN_GATEWAY_IP=
LAN_TARGET_IP=
NETWORK_ID=
VIRTUAL_IP_A=
VIRTUAL_IP_B=
NODE_ID_C=

if [[ $CALIBRATION == 1 ]]; then
    [[ $DURATION_SECONDS =~ ^[0-9]+$ && $DURATION_SECONDS -ge 600 ]]
else
    [[ $DURATION_SECONDS =~ ^[0-9]+$ && $DURATION_SECONDS -ge 86400 ]]
fi
[[ $SAMPLE_INTERVAL_SECONDS =~ ^[0-9]+$ && $SAMPLE_INTERVAL_SECONDS -ge 1 ]]
[[ $(id -u) -eq 0 ]]
[[ -c /dev/net/tun ]]

require_command() {
    command -v "$1" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$1" >&2
        exit 2
    }
}

for command in cargo curl docker find git ip nft nsenter openssl ping python3 sha256sum ss systemctl tc; do
    require_command "$command"
done
docker compose version >/dev/null

mkdir -p "$EVIDENCE_DIR"
chmod 0700 "$EVIDENCE_DIR"
umask 077

compose() {
    docker compose --env-file "$ENVIRONMENT_FILE" \
        -f "$COMPOSE_FILE" -f "$SOAK_COMPOSE_FILE" "$@"
}

nft_snapshot() {
    nft -j list ruleset | python3 -c '
import json
import sys


def normalize(value):
    if isinstance(value, dict):
        normalized = {key: normalize(item) for key, item in value.items()}
        counter = normalized.get("counter")
        if isinstance(counter, dict):
            counter.pop("packets", None)
            counter.pop("bytes", None)
        return normalized
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


json.dump(normalize(json.load(sys.stdin)), sys.stdout, sort_keys=True, separators=(",", ":"))
'
}

capture_onepanel() {
    local output=$1
    if docker network inspect 1panel-network >/dev/null 2>&1; then
        {
            docker network inspect 1panel-network --format '{{.Id}}'
            docker network inspect 1panel-network --format '{{json .IPAM.Config}}'
            docker network inspect 1panel-network --format '{{json .Containers}}'
        } >"$output"
    else
        printf 'absent\n' >"$output"
    fi
}

capture_host_baseline() {
    local prefix=$1
    docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort >"$prefix-docker-networks.txt"
    docker volume ls --format '{{.Name}} {{.Driver}}' | sort >"$prefix-docker-volumes.txt"
    docker ps -a --format '{{.ID}} {{.Names}} {{.Image}} {{.Status}}' | sort >"$prefix-docker-containers.txt"
    ip -json route show default >"$prefix-default-routes.json"
    ip -json rule show >"$prefix-rules.json"
    ip -json link show | python3 -c 'import json,sys; print("\n".join(sorted(x["ifname"] for x in json.load(sys.stdin))))' >"$prefix-links.txt"
    nft_snapshot >"$prefix-nftables.json"
    systemctl --failed --no-legend --plain 2>/dev/null | sort >"$prefix-failed-services.txt" || true
    capture_onepanel "$prefix-onepanel-network.txt"
}

record_logs() {
    local container service
    for service in controller relay console postgres; do
        container=$(compose ps -q "$service" 2>/dev/null || true)
        if [[ -n $container ]]; then
            docker logs "$container" >"$EVIDENCE_DIR/$service.log" 2>&1 || true
        fi
    done
    for container in "$AGENT_A" "$AGENT_B" "$AGENT_C" \
        "$AGENT_A_NETNS" "$AGENT_B_NETNS" "$AGENT_C_NETNS" "$LAN_TARGET"; do
        if docker inspect "$container" >/dev/null 2>&1; then
            docker logs "$container" >"$EVIDENCE_DIR/$container.log" 2>&1 || true
        fi
    done
}

clear_agent_faults() {
    local container pid device
    for container in "$AGENT_A" "$AGENT_B"; do
        if docker inspect "$container" >/dev/null 2>&1; then
            pid=$(docker inspect "$container" --format '{{.State.Pid}}')
            if [[ $pid =~ ^[0-9]+$ && $pid -gt 0 ]]; then
                nsenter -t "$pid" -n nft delete table inet xs_gate22_direct >/dev/null 2>&1 || true
                device=$(nsenter -t "$pid" -n ip -o route get "$CONTROLLER_IP" 2>/dev/null |
                    awk '{for (index=1; index<=NF; index++) if ($index == "dev") {print $(index+1); exit}}')
                if [[ -n $device ]]; then
                    nsenter -t "$pid" -n tc qdisc del dev "$device" root >/dev/null 2>&1 || true
                fi
            fi
        fi
    done
}

remove_resources() {
    clear_agent_faults
    docker rm -f "$AGENT_A" "$AGENT_B" "$AGENT_C" >/dev/null 2>&1 || true
    docker rm -f "$AGENT_A_NETNS" "$AGENT_B_NETNS" "$AGENT_C_NETNS" "$LAN_TARGET" \
        >/dev/null 2>&1 || true
    if [[ -f $ENVIRONMENT_FILE ]]; then
        compose down --volumes --remove-orphans >/dev/null 2>&1 || true
    fi
    docker network rm "$LAN_NETWORK" >/dev/null 2>&1 || true
    docker network rm "$NETWORK" >/dev/null 2>&1 || true
    docker network rm "$PRIME_NETWORK" >/dev/null 2>&1 || true
}

finalize_evidence() {
    local status=$1 manifest secret_status=0
    record_logs
    {
        uname -a
        docker version
        docker compose version
        rustc --version --verbose
        cargo --version
        python3 --version
        nft --version
        ip -Version
        tc -Version
    } >"$EVIDENCE_DIR/environment.txt" 2>&1 || status=1
    python3 "$ROOT_DIR/scripts/check-secrets.py" --root "$EVIDENCE_DIR" \
        >"$EVIDENCE_DIR/secret-scan.log" 2>&1 || secret_status=$?
    (( secret_status == 0 )) || status=1
    manifest=$(mktemp "$TEMPORARY/manifest.XXXXXX")
    (
        cd "$EVIDENCE_DIR"
        find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum >"$manifest"
    )
    mv "$manifest" "$EVIDENCE_DIR/SHA256SUMS"
    return "$status"
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    set +e
    record_logs
    remove_resources
    capture_host_baseline "$EVIDENCE_DIR/host-after"
    for suffix in docker-networks.txt docker-volumes.txt docker-containers.txt default-routes.json rules.json links.txt nftables.json failed-services.txt onepanel-network.txt; do
        if ! cmp -s "$EVIDENCE_DIR/host-before-$suffix" "$EVIDENCE_DIR/host-after-$suffix"; then
            printf 'host baseline changed: %s\n' "$suffix" >&2
            status=1
        fi
    done
    finalize_evidence "$status"
    status=$?
    rm -rf "$TEMPORARY"
    exit "$status"
}
trap cleanup EXIT INT TERM

select_subnets() {
    local used candidate selected=()
    used=$(docker network ls -q | xargs -r docker network inspect \
        --format '{{range .IPAM.Config}}{{println .Subnet}}{{end}}' 2>/dev/null || true)
    for third in $(seq 200 250); do
        candidate="172.29.$third.0/24"
        if ! grep -Fxq "$candidate" <<<"$used" && ! ip route show | grep -Fq "172.29.$third.0/24"; then
            selected+=("$candidate")
            (( ${#selected[@]} == 2 )) && break
        fi
    done
    (( ${#selected[@]} == 2 ))
    UNDERLAY_SUBNET=${selected[0]}
    LAN_SUBNET=${selected[1]}
    local underlay_prefix=${UNDERLAY_SUBNET%0/24}
    local lan_prefix=${LAN_SUBNET%0/24}
    CONTROLLER_IP="${underlay_prefix}10"
    RELAY_IP="${underlay_prefix}11"
    POSTGRES_IP="${underlay_prefix}12"
    CONSOLE_IP="${underlay_prefix}13"
    AGENT_A_IP="${underlay_prefix}21"
    AGENT_B_IP="${underlay_prefix}22"
    AGENT_C_IP="${underlay_prefix}23"
    LAN_GATEWAY_IP="${lan_prefix}10"
    LAN_TARGET_IP="${lan_prefix}2"
}

select_ports() {
    mapfile -t ports < <(python3 - <<'PY'
import socket


def port(kind):
    sock_type = socket.SOCK_DGRAM if kind == "udp" else socket.SOCK_STREAM
    with socket.socket(socket.AF_INET, sock_type) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


used = set()
for kind in ("tcp", "tcp", "udp", "udp"):
    while True:
        value = port(kind)
        if value not in used:
            used.add(value)
            print(value)
            break
PY
)
    CONTROLLER_PORT=${ports[0]}
    CONSOLE_PORT=${ports[1]}
    DISCOVERY_PORT=${ports[2]}
    RELAY_PORT=${ports[3]}
}

wait_for_http() {
    local url=$1 label=$2
    for _attempt in $(seq 1 120); do
        if curl --fail --silent --show-error "$url" >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done
    printf '%s did not become ready: %s\n' "$label" "$url" >&2
    return 1
}

wait_for_postgres() {
    local container
    container=$(compose ps -q postgres)
    for _attempt in $(seq 1 120); do
        if docker exec "$container" pg_isready -q -h 127.0.0.1 -U gate22_bootstrap -d gate22; then
            return
        fi
        sleep 1
    done
    printf 'PostgreSQL did not become ready\n' >&2
    return 1
}

admin_request() {
    local method=$1 path=$2 output=$3 data_file=${4:-}
    local -a arguments=(
        --fail --silent --show-error
        --config "$CONTROLLER_SECRETS/admin-curl.conf"
        -X "$method"
        "http://127.0.0.1:$CONTROLLER_PORT$path"
        -H 'Content-Type: application/json'
        -o "$output"
    )
    if [[ -n $data_file ]]; then
        arguments+=(--data-binary "@$data_file")
    fi
    curl "${arguments[@]}"
}

write_environment() {
    local revision source_epoch
    revision=$(git -c safe.directory="$ROOT_DIR" -C "$ROOT_DIR" rev-parse HEAD)
    source_epoch=$(git -c safe.directory="$ROOT_DIR" -C "$ROOT_DIR" show -s --format=%ct HEAD)
    cat >"$ENVIRONMENT_FILE" <<EOF
XS_DEPLOYMENT=$DEPLOYMENT
XS_COMPOSE_PROJECT_NAME=$PROJECT
XS_RELEASE_REVISION=$revision
XS_RELEASE_VERSION=0.1.0-gate22
SOURCE_DATE_EPOCH=$source_epoch
XS_CONTROLLER_IMAGE=xs-nexus/controller:gate22-$revision
XS_MIGRATION_IMAGE=xs-nexus/controller:gate22-$revision
XS_RELAY_IMAGE=xs-nexus/relay:gate22-$revision
XS_CONSOLE_IMAGE=xs-nexus/console:gate22-$revision
XS_DB_TOOLS_IMAGE=xs-nexus/db-tools:gate22-$revision
XS_CONTROLLER_SECRETS_DIR=$CONTROLLER_SECRETS
XS_DATABASE_SECRETS_DIR=$DATABASE_SECRETS
XS_GATE22_POSTGRES_SECRETS_DIR=$POSTGRES_SECRETS
XS_GATE22_REPOSITORY_ROOT=$ROOT_DIR
XS_GATE22_NETWORK=$NETWORK
XS_GATE22_CONTROLLER_IP=$CONTROLLER_IP
XS_GATE22_RELAY_IP=$RELAY_IP
XS_GATE22_POSTGRES_IP=$POSTGRES_IP
XS_GATE22_CONSOLE_IP=$CONSOLE_IP
XS_LINUX_RELEASE_DIR=$RELEASE_DIRECTORY
XS_WINDOWS_RELEASE_DIR=$WINDOWS_RELEASE_DIRECTORY
XS_RELAY_SECRETS_DIR=$RELAY_SECRETS
XS_BACKUP_DIR=$BACKUP_DIRECTORY
XS_BACKUP_REPLICA_DIR=$REPLICA_DIRECTORY
XS_BACKUP_LOCAL_RETENTION_DAYS=30
XS_BACKUP_REPLICA_RETENTION_DAYS=180
XS_BACKUP_MIN_RETAINED=3
XS_STATE_DIR=$STATE_DIRECTORY
XS_DATABASE_SCHEMA=$DATABASE_SCHEMA
XS_DATABASE_APP_ROLE=$DATABASE_APP_ROLE
XS_DATABASE_OWNER_ROLE=$DATABASE_OWNER_ROLE
XS_CONSOLE_BOOTSTRAP_USERNAME=admin
XS_CONSOLE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/xs-controller/console-bootstrap-password
XS_BIND_ADDRESS=127.0.0.1
XS_UDP_BIND_ADDRESS=127.0.0.1
XS_CONTROLLER_HTTP_PORT=$CONTROLLER_PORT
XS_CONSOLE_HTTP_PORT=$CONSOLE_PORT
XS_DISCOVERY_UDP_PORT=$DISCOVERY_PORT
XS_RELAY_UDP_PORT=$RELAY_PORT
XS_DISCOVERY_PUBLIC_ENDPOINT=$CONTROLLER_IP:42000
XS_RELAY_ID_BASE64=$RELAY_ID_BASE64
XS_CONSOLE_COOKIE_SECURE=false
XS_NODE_CREDENTIAL_TTL_SECONDS=172800
XS_RUST_LOG=info
EOF
    chmod 0600 "$ENVIRONMENT_FILE"
}

prepare_secrets() {
    mkdir -p "$CONTROLLER_SECRETS" "$DATABASE_SECRETS" "$POSTGRES_SECRETS" \
        "$RELAY_SECRETS" "$RELEASE_DIRECTORY" "$WINDOWS_RELEASE_DIRECTORY" \
        "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY" "$STATE_DIRECTORY" "$BINARY_DIRECTORY"
    chmod 0700 "$CONTROLLER_SECRETS" "$DATABASE_SECRETS" "$POSTGRES_SECRETS" \
        "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY" "$STATE_DIRECTORY"
    chmod 0755 "$RELEASE_DIRECTORY" "$WINDOWS_RELEASE_DIRECTORY" "$BINARY_DIRECTORY"

    openssl rand -hex 32 >"$POSTGRES_SECRETS/bootstrap-password"
    openssl rand -hex 32 >"$POSTGRES_SECRETS/app-password"
    openssl rand -hex 32 >"$POSTGRES_SECRETS/migrator-password"
    local app_password migrator_password admin_token console_password database_scheme
    app_password=$(<"$POSTGRES_SECRETS/app-password")
    migrator_password=$(<"$POSTGRES_SECRETS/migrator-password")
    admin_token=$(openssl rand -hex 32)
    console_password=$(openssl rand -base64 24 | tr -d '\n')
    database_scheme=postgresql
    printf '%s://%s:%s@%s:5432/gate22' \
        "$database_scheme" "$DATABASE_APP_ROLE" "$app_password" "$POSTGRES_IP" \
        >"$CONTROLLER_SECRETS/database-url"
    printf '%s://%s:%s@%s:5432/gate22' \
        "$database_scheme" "$DATABASE_MIGRATOR_ROLE" "$migrator_password" "$POSTGRES_IP" \
        >"$DATABASE_SECRETS/database-url"
    printf '%s' "$admin_token" >"$CONTROLLER_SECRETS/admin-api-token"
    printf 'header = "Authorization: Bearer %s"\n' "$admin_token" >"$CONTROLLER_SECRETS/admin-curl.conf"
    printf '%s' "$console_password" >"$CONTROLLER_SECRETS/console-bootstrap-password"
    unset app_password migrator_password admin_token console_password database_scheme

    head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/credential-signing-key"
    head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/configuration-signing-key"
    head -c 32 /dev/urandom >"$RELAY_SECRETS/identity-key"
    printf '%s\n' 'age1wcd4cep4z26php4gteja7n2wgxyukpe4nc5xn8dr6zk85gg26chsy9qyj8' \
        >"$CONTROLLER_SECRETS/backup-recipient"
    printf '%s\n' 'gate22-public-key-placeholder' >"$RELEASE_DIRECTORY/release-public-key.pem"
    for architecture in x86_64 aarch64; do
        target="$architecture-unknown-linux-gnu"
        printf '%s\n' 'gate22-manifest-placeholder' \
            >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.manifest"
        head -c 64 /dev/urandom \
            >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.manifest.sig"
        printf '%s\n' 'gate22-archive-placeholder' \
            >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.tar.gz"
    done
    printf '%s\n' 'Write-Output gate22-calibration' >"$WINDOWS_RELEASE_DIRECTORY/install.ps1"
    printf '%s\n' '{"schema_version":1}' \
        >"$WINDOWS_RELEASE_DIRECTORY/xs-nexus-0.1.0-x86_64-pc-windows-msvc.manifest.json"
    printf '%s\n' 'gate22-placeholder' \
        >"$WINDOWS_RELEASE_DIRECTORY/xs-nexus-0.1.0-x86_64-pc-windows-msvc.zip"
    chmod 0644 "$RELEASE_DIRECTORY"/* "$WINDOWS_RELEASE_DIRECTORY"/*

    "$BINARY_DIRECTORY/derive_ed25519_public" \
        "$CONTROLLER_SECRETS/credential-signing-key" \
        "$RELAY_SECRETS/controller-credential-public-key"
    "$BINARY_DIRECTORY/derive_ed25519_public" \
        "$CONTROLLER_SECRETS/configuration-signing-key" \
        "$CONTROLLER_SECRETS/update-signing-public-key"
    "$BINARY_DIRECTORY/derive_ed25519_public" \
        "$RELAY_SECRETS/identity-key" "$TEMPORARY/relay-public-key"
    RELAY_ID_BASE64=$(python3 -c 'import base64,os; print(base64.urlsafe_b64encode(os.urandom(16)).rstrip(b"=").decode())')
    python3 - "$CONTROLLER_SECRETS/relay-catalog.json" "$RELAY_ID_BASE64" \
        "$RELAY_IP:42001" "$TEMPORARY/relay-public-key" <<'PY'
import base64
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

catalog = [{
    "relay_id_base64": sys.argv[2],
    "endpoint": sys.argv[3],
    "identity_public_key_base64": base64.urlsafe_b64encode(
        Path(sys.argv[4]).read_bytes()
    ).rstrip(b"=").decode(),
    "priority": 100,
    "expires_at": (datetime.now(timezone.utc) + timedelta(days=3)).isoformat().replace("+00:00", "Z"),
}]
Path(sys.argv[1]).write_text(json.dumps(catalog), encoding="utf-8")
PY
    rm -f "$TEMPORARY/relay-public-key"

    local postgres_uid
    postgres_uid=$(docker run --rm --entrypoint id "$POSTGRES_IMAGE" -u postgres)
    chown -R 65532:65532 "$CONTROLLER_SECRETS" "$DATABASE_SECRETS" "$RELAY_SECRETS" \
        "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY"
    chown -R "$postgres_uid:$postgres_uid" "$POSTGRES_SECRETS"
    find "$CONTROLLER_SECRETS" "$DATABASE_SECRETS" "$POSTGRES_SECRETS" "$RELAY_SECRETS" \
        -type f -exec chmod 0400 {} +
}

initialize_database() {
    compose up -d postgres
    wait_for_postgres
    local postgres_container
    postgres_container=$(compose ps -q postgres)
    docker exec -u postgres "$postgres_container" sh -eu -c '
        export PGPASSWORD=$(cat /run/secrets/xs-gate22/bootstrap-password)
        export XS_DATABASE_APP_PASSWORD=$(cat /run/secrets/xs-gate22/app-password)
        export XS_DATABASE_MIGRATOR_PASSWORD=$(cat /run/secrets/xs-gate22/migrator-password)
        exec psql -q -h 127.0.0.1 -U gate22_bootstrap -d gate22 \
            -v schema=gate22_schema -v owner_role=gate22_owner \
            -v app_role=gate22_app -v migrator_role=gate22_migrator \
            -f /workspace/deploy/docker/postgres-role-hardening.sql
    ' >"$EVIDENCE_DIR/postgres-role-hardening.log"
    compose --profile migration run --rm migration >"$EVIDENCE_DIR/migration.log" 2>&1
}

start_application() {
    compose up -d controller relay console
    wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready" Controller
    wait_for_http "http://127.0.0.1:$CONSOLE_PORT/console-health" Console
    local relay_container
    relay_container=$(compose ps -q relay)
    for _attempt in $(seq 1 120); do
        if docker exec "$relay_container" /usr/local/bin/xs-relay healthcheck >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done
    printf 'Relay did not become ready\n' >&2
    return 1
}

create_network_and_acl() {
    cat >"$TEMPORARY/network.json" <<EOF
{"name":"gate22-$SUFFIX","address_pool":"100.115.22.0/24","reserved_addresses":16}
EOF
    admin_request POST /v1/admin/networks "$TEMPORARY/network-response.json" "$TEMPORARY/network.json"
    NETWORK_ID=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["id"])' \
        "$TEMPORARY/network-response.json")
    cat >"$TEMPORARY/acl-initial.json" <<'EOF'
{
  "expected_policy_version": 1,
  "groups": [],
  "rules": [
    {
      "id": "allow-soak-traffic",
      "priority": 100,
      "action": "allow",
      "sources": [{"type": "any"}],
      "destinations": [{"type": "any"}],
      "protocol": "any",
      "destination_ports": []
    }
  ]
}
EOF
    admin_request PUT "/v1/admin/networks/$NETWORK_ID/acl" \
        "$TEMPORARY/acl-initial-response.json" "$TEMPORARY/acl-initial.json"
}

create_enrollment_token() {
    local output=$1
    cat >"$TEMPORARY/enrollment-request.json" <<EOF
{"network_id":"$NETWORK_ID","expires_in_seconds":3600,"max_uses":1,"default_role_bitmap":1,"default_tags":["linux","gate22"],"requested_virtual_ip":null}
EOF
    admin_request POST /v1/admin/enrollment-tokens "$TEMPORARY/enrollment-response.json" \
        "$TEMPORARY/enrollment-request.json"
    python3 - "$TEMPORARY/enrollment-response.json" "$output" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[2]).write_text(json.load(open(sys.argv[1], encoding="utf-8"))["token"], encoding="utf-8")
os.chmod(sys.argv[2], 0o400)
PY
}

write_agent_config() {
    local root=$1 name=$2 interface=$3
    mkdir -p "$root/state"
    python3 - "$root/agent.json" "$name" "$interface" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "controller_url": "http://127.0.0.1:8080/",
    "node_name": sys.argv[2],
    "device_type": "linux",
    "state_directory": "/agent/state",
    "runtime_directory": "/run/xs-agent",
    "interface_name": sys.argv[3],
    "mtu": 1280,
    "control_sync_interval_seconds": 5,
}), encoding="utf-8")
PY
    chown -R 65532:65532 "$root"
    chmod 0700 "$root" "$root/state"
    chmod 0400 "$root/agent.json"
}

prepare_agent_proxy() {
    cat >"$AGENT_PROXY_SCRIPT" <<EOF
#!/bin/sh
exec nc "$CONTROLLER_IP" 8080
EOF
    chmod 0555 "$AGENT_PROXY_SCRIPT"
}

start_agent_netns() {
    local container=$1 address=$2
    docker run -d --name "$container" \
        --label "com.xs-nexus.gate22.project=$PROJECT" \
        --network "$NETWORK" --ip "$address" \
        --user 65532:65532 \
        --read-only --cap-drop ALL \
        --security-opt no-new-privileges:true \
        --pids-limit 64 \
        --mount "type=bind,src=$AGENT_PROXY_SCRIPT,dst=/proxy-upstream,readonly" \
        "$ALPINE_IMAGE" nc -ll -p 8080 -e /proxy-upstream >/dev/null
}

wait_agent_proxy() {
    local container=$1
    for _attempt in $(seq 1 60); do
        if docker exec "$container" wget -q -T 2 -O /dev/null \
            http://127.0.0.1:8080/health/live; then
            return
        fi
        sleep 1
    done
    printf 'Agent loopback Controller proxy did not become ready: %s\n' "$container" >&2
    return 1
}

agent_run_base() {
    local root=$1 netns=$2
    printf '%s\n' \
        --user 65532:65532 \
        --network "container:$netns" \
        --mount "type=bind,src=$BINARY_DIRECTORY,dst=/workspace,readonly" \
        --mount "type=bind,src=$root,dst=/agent" \
        --tmpfs /run/xs-agent:rw,noexec,nosuid,nodev,uid=65532,gid=65532,mode=0700 \
        --tmpfs /tmp:rw,noexec,nosuid,nodev,mode=1777
}

enroll_agent() {
    local root=$1 netns=$2
    create_enrollment_token "$root/enrollment.token"
    chown 65532:65532 "$root/enrollment.token"
    local -a arguments
    mapfile -t arguments < <(agent_run_base "$root" "$netns")
    docker run --rm "${arguments[@]}" --entrypoint /workspace/xs-agent "$AGENT_RUNTIME_IMAGE" \
        enroll --config /agent/agent.json --token-file /agent/enrollment.token >/dev/null
    rm -f "$root/enrollment.token"
}

start_agent() {
    local container=$1 root=$2 netns=$3
    local -a arguments
    mapfile -t arguments < <(agent_run_base "$root" "$netns")
    docker run -d --name "$container" \
        --label "com.xs-nexus.gate22.project=$PROJECT" \
        --cap-drop ALL --cap-add NET_ADMIN \
        --security-opt no-new-privileges:true \
        --pids-limit 256 \
        --device /dev/net/tun:/dev/net/tun \
        "${arguments[@]}" \
        --entrypoint /workspace/xs-agent "$AGENT_RUNTIME_IMAGE" \
        run --config /agent/agent.json >/dev/null
}

agent_cli() {
    local container=$1
    shift
    docker exec "$container" /workspace/xs "$@" --socket /run/xs-agent/agent.sock --json
}

wait_agent_ready() {
    local container=$1 output
    for _attempt in $(seq 1 180); do
        output=$(agent_cli "$container" status 2>/dev/null || true)
        if [[ -n $output ]] && python3 -c '
import json,sys
status=json.load(sys.stdin)["status"]
raise SystemExit(0 if status["controller_connected"] and status["network_active"] else 1)
' <<<"$output"; then
            return
        fi
        sleep 1
    done
    printf 'Agent did not become ready: %s\n' "$container" >&2
    docker inspect "$container" --format '{{json .State}}' >&2 || true
    docker logs "$container" >&2 || true
    return 1
}

wait_configuration_version() {
    local container=$1 expected=$2 output
    for _attempt in $(seq 1 180); do
        output=$(agent_cli "$container" status 2>/dev/null || true)
        if [[ -n $output ]] && python3 -c '
import json
import sys
status = json.load(sys.stdin)["status"]
raise SystemExit(0 if status["configuration_version"] >= int(sys.argv[1]) else 1)
' "$expected" <<<"$output"
        then
            return
        fi
        sleep 1
    done
    printf 'Agent did not apply configuration version %s: %s\n' "$expected" "$container" >&2
    return 1
}

ping_agent() {
    local container=$1 virtual_ip=$2 output
    output=$(agent_cli "$container" ping "$virtual_ip")
    python3 -c '
import json,sys
result=json.load(sys.stdin)["result"]
raise SystemExit(0 if result["reachable"] and result["error_code"] is None else 1)
' <<<"$output"
}

wait_path_kind() {
    local container=$1 virtual_ip=$2 expected=$3 output
    for _attempt in $(seq 1 180); do
        output=$(agent_cli "$container" path "$virtual_ip" 2>/dev/null || true)
        if [[ -n $output ]] && python3 -c '
import json
import sys
path = json.load(sys.stdin)["path"]
expected = sys.argv[1]
kind = path["active_candidate_kind"]
matches = kind == "relay" if expected == "relay" else kind in {"local", "stun"}
raise SystemExit(0 if path["session_established"] and matches else 1)
' "$expected" <<<"$output"
        then
            return
        fi
        ping_agent "$container" "$virtual_ip" >/dev/null 2>&1 || true
        sleep 1
    done
    printf 'Agent path did not become %s: %s\n' "$expected" "$container" >&2
    return 1
}

prepare_agents() {
    prepare_agent_proxy
    write_agent_config "$AGENT_A_ROOT" gate22-agent-a xsga0
    write_agent_config "$AGENT_B_ROOT" gate22-agent-b xsgb0
    start_agent_netns "$AGENT_A_NETNS" "$AGENT_A_IP"
    start_agent_netns "$AGENT_B_NETNS" "$AGENT_B_IP"
    wait_agent_proxy "$AGENT_A_NETNS"
    wait_agent_proxy "$AGENT_B_NETNS"
    enroll_agent "$AGENT_A_ROOT" "$AGENT_A_NETNS"
    enroll_agent "$AGENT_B_ROOT" "$AGENT_B_NETNS"
    start_agent "$AGENT_A" "$AGENT_A_ROOT" "$AGENT_A_NETNS"
    start_agent "$AGENT_B" "$AGENT_B_ROOT" "$AGENT_B_NETNS"
    docker network connect --ip "$LAN_GATEWAY_IP" "$LAN_NETWORK" "$AGENT_B_NETNS"
    docker run -d --rm --name "$LAN_TARGET" --network "$LAN_NETWORK" --ip "$LAN_TARGET_IP" \
        --user 65534:65534 --read-only --cap-drop ALL --security-opt no-new-privileges:true \
        "$ALPINE_IMAGE" sleep 172800 >/dev/null
    wait_agent_ready "$AGENT_A"
    wait_agent_ready "$AGENT_B"
    VIRTUAL_IP_A=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' \
        "$AGENT_A_ROOT/state/node-state.json")
    VIRTUAL_IP_B=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["virtual_ip"])' \
        "$AGENT_B_ROOT/state/node-state.json")
    wait_path_kind "$AGENT_A" "$VIRTUAL_IP_B" direct
    wait_path_kind "$AGENT_B" "$VIRTUAL_IP_A" direct
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
    ping_agent "$AGENT_B" "$VIRTUAL_IP_A"
}

container_metrics() {
    local container=$1 root_pid
    root_pid=$(docker inspect "$container" --format '{{.State.Pid}}')
    python3 - "$root_pid" <<'PY'
import sys
from pathlib import Path

root = int(sys.argv[1])
parents = {}
for entry in Path("/proc").iterdir():
    if not entry.name.isdigit():
        continue
    try:
        stat = (entry / "stat").read_text().rsplit(") ", 1)[1].split()
        parents[int(entry.name)] = int(stat[1])
    except (FileNotFoundError, IndexError, PermissionError, ProcessLookupError, ValueError):
        pass

pids = {root}
changed = True
while changed:
    changed = False
    for pid, parent in parents.items():
        if parent in pids and pid not in pids:
            pids.add(pid)
            changed = True

rss_kib = threads = fds = cpu_ticks = 0
for pid in pids:
    proc = Path("/proc") / str(pid)
    try:
        values = {}
        for line in (proc / "status").read_text().splitlines():
            if ":" in line:
                key, value = line.split(":", 1)
                if key in {"VmRSS", "Threads"}:
                    values[key] = value.strip().split()[0]
        rss_kib += int(values.get("VmRSS", 0))
        threads += int(values.get("Threads", 0))
        fds += len(list((proc / "fd").iterdir()))
        stat = (proc / "stat").read_text().rsplit(") ", 1)[1].split()
        cpu_ticks += int(stat[11]) + int(stat[12])
    except (FileNotFoundError, IndexError, PermissionError, ProcessLookupError, ValueError):
        pass

print(root, rss_kib, threads, fds, cpu_ticks)
PY
}

container_log_bytes() {
    local container=$1 log_path directory name
    log_path=$(docker inspect "$container" --format '{{.LogPath}}')
    if [[ -z $log_path ]]; then
        printf '0\n'
        return
    fi
    directory=$(dirname "$log_path")
    name=$(basename "$log_path")
    find "$directory" -maxdepth 1 -type f -name "$name*" -printf '%s\n' 2>/dev/null |
        awk '{total += $1} END {print total + 0}'
}

agent_network_counts() {
    local container=$1 pid
    pid=$(docker inspect "$container" --format '{{.State.Pid}}')
    printf '%s %s %s\n' \
        "$(nsenter -t "$pid" -n ip -4 route show table all | wc -l)" \
        "$(nsenter -t "$pid" -n ip rule show | wc -l)" \
        "$(nsenter -t "$pid" -n ip -o link show | wc -l)"
}

agent_path_sample() {
    local container=$1 virtual_ip=$2 output
    output=$(agent_cli "$container" path "$virtual_ip")
    python3 -c '
import json,sys
path=json.load(sys.stdin)["path"]
if not path["session_established"] or path["active_candidate_kind"] is None:
    raise SystemExit(1)
print(path["active_candidate_kind"], path["tx_packets_total"], path["rx_packets_total"])
' <<<"$output"
}

sample_container() {
    local timestamp=$1 epoch=$2 service=$3 container=$4 peer_ip=${5:-}
    local root_pid rss_kib threads fds cpu_ticks log_bytes restarts
    local route_count='' rule_count='' interface_count='' db_connections='' path_kind='' tx_packets='' rx_packets=''
    read -r root_pid rss_kib threads fds cpu_ticks < <(container_metrics "$container")
    log_bytes=$(container_log_bytes "$container")
    restarts=$(docker inspect "$container" --format '{{.RestartCount}}')
    if [[ $service == postgres ]]; then
        db_connections=$(docker exec -u postgres "$container" psql -At -U gate22_bootstrap -d gate22 \
            -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'gate22'" | tr -d '[:space:]')
    elif [[ $service == agent-* ]]; then
        read -r route_count rule_count interface_count < <(agent_network_counts "$container")
        read -r path_kind tx_packets rx_packets < <(agent_path_sample "$container" "$peer_ip")
    fi
    printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
        "$timestamp" "$epoch" "$service" "$container" "$root_pid" "$rss_kib" "$threads" \
        "$fds" "$cpu_ticks" "$log_bytes" "$restarts" "$route_count" "$rule_count" \
        "$interface_count" "$db_connections" "$path_kind" "$tx_packets" "$rx_packets" \
        >>"$EVIDENCE_DIR/resources.csv"
}

sample_all() {
    local timestamp epoch
    timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    epoch=$(date +%s)
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
    ping_agent "$AGENT_B" "$VIRTUAL_IP_A"
    sample_container "$timestamp" "$epoch" controller "$(compose ps -q controller)"
    sample_container "$timestamp" "$epoch" relay "$(compose ps -q relay)"
    sample_container "$timestamp" "$epoch" console "$(compose ps -q console)"
    sample_container "$timestamp" "$epoch" postgres "$(compose ps -q postgres)"
    sample_container "$timestamp" "$epoch" agent-a "$AGENT_A" "$VIRTUAL_IP_B"
    sample_container "$timestamp" "$epoch" agent-b "$AGENT_B" "$VIRTUAL_IP_A"
    admin_request GET /v1/admin/observability "$TEMPORARY/observability.json"
    python3 - "$timestamp" "$TEMPORARY/observability.json" >>"$EVIDENCE_DIR/observability.jsonl" <<'PY'
import json
import sys

print(json.dumps({"timestamp_utc": sys.argv[1], "snapshot": json.load(open(sys.argv[2], encoding="utf-8"))}, sort_keys=True))
PY
}

restart_compose_service() {
    local service=$1
    local container output
    container=$(compose ps -q "$service")
    output=$(docker restart --timeout 20 "$container")
    [[ $output == "$container" ]]
}

fault_controller_restart() {
    restart_compose_service controller
    wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready" Controller
    wait_agent_ready "$AGENT_A"
    wait_agent_ready "$AGENT_B"
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
}

block_direct() {
    local container peer_ip pid
    while (( $# > 0 )); do
        container=$1
        peer_ip=$2
        shift 2
        pid=$(docker inspect "$container" --format '{{.State.Pid}}')
        nsenter -t "$pid" -n nft add table inet xs_gate22_direct
        nsenter -t "$pid" -n nft add chain inet xs_gate22_direct output \
            '{ type filter hook output priority 0; policy accept; }'
        nsenter -t "$pid" -n nft add rule inet xs_gate22_direct output \
            ip daddr "$peer_ip" meta l4proto udp drop
    done
}

unblock_direct() {
    local container pid
    for container in "$AGENT_A" "$AGENT_B"; do
        pid=$(docker inspect "$container" --format '{{.State.Pid}}')
        nsenter -t "$pid" -n nft delete table inet xs_gate22_direct
    done
}

fault_relay_restart() {
    block_direct "$AGENT_A" "$AGENT_B_IP" "$AGENT_B" "$AGENT_A_IP"
    wait_path_kind "$AGENT_A" "$VIRTUAL_IP_B" relay
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
    restart_compose_service relay
    local relay_container
    relay_container=$(compose ps -q relay)
    for _attempt in $(seq 1 120); do
        if docker exec "$relay_container" /usr/local/bin/xs-relay healthcheck >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    docker exec "$relay_container" /usr/local/bin/xs-relay healthcheck >/dev/null
    wait_path_kind "$AGENT_A" "$VIRTUAL_IP_B" relay
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
    unblock_direct
    wait_path_kind "$AGENT_A" "$VIRTUAL_IP_B" direct
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
}

fault_postgres_restart() {
    restart_compose_service postgres
    wait_for_postgres
    wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready" Controller
    wait_agent_ready "$AGENT_A"
    wait_agent_ready "$AGENT_B"
    ping_agent "$AGENT_B" "$VIRTUAL_IP_A"
}

fault_agent_restart() {
    local output
    output=$(docker restart --timeout 20 "$AGENT_A")
    [[ $output == "$AGENT_A" ]]
    wait_agent_ready "$AGENT_A"
    wait_path_kind "$AGENT_A" "$VIRTUAL_IP_B" direct
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
}

fault_configuration_update() {
    cat >"$TEMPORARY/acl-update.json" <<'EOF'
{
  "expected_policy_version": 2,
  "groups": [],
  "rules": [
    {
      "id": "deny-unused-soak-port",
      "priority": 200,
      "action": "deny",
      "sources": [{"type": "any"}],
      "destinations": [{"type": "any"}],
      "protocol": "udp",
      "destination_ports": [{"start": 49999, "end": 49999}]
    },
    {
      "id": "allow-soak-traffic",
      "priority": 100,
      "action": "allow",
      "sources": [{"type": "any"}],
      "destinations": [{"type": "any"}],
      "protocol": "any",
      "destination_ports": []
    }
  ]
}
EOF
    admin_request PUT "/v1/admin/networks/$NETWORK_ID/acl" \
        "$TEMPORARY/acl-update-response.json" "$TEMPORARY/acl-update.json"
    local version
    version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["configuration_version"])' \
        "$TEMPORARY/acl-update-response.json")
    wait_configuration_version "$AGENT_A" "$version"
    wait_configuration_version "$AGENT_B" "$version"
    ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
}

fault_enrollment_revocation() {
    write_agent_config "$AGENT_C_ROOT" gate22-agent-c xsgc0
    start_agent_netns "$AGENT_C_NETNS" "$AGENT_C_IP"
    wait_agent_proxy "$AGENT_C_NETNS"
    enroll_agent "$AGENT_C_ROOT" "$AGENT_C_NETNS"
    start_agent "$AGENT_C" "$AGENT_C_ROOT" "$AGENT_C_NETNS"
    wait_agent_ready "$AGENT_C"
    NODE_ID_C=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["node_id_base64"])' \
        "$AGENT_C_ROOT/state/node-state.json")
    cat >"$TEMPORARY/revoke.json" <<'EOF'
{"ip_cooldown_seconds":60}
EOF
    admin_request POST "/v1/admin/networks/$NETWORK_ID/nodes/$NODE_ID_C/revoke" \
        "$TEMPORARY/revoke-response.json" "$TEMPORARY/revoke.json"
    local rejected=0 output
    for _attempt in $(seq 1 60); do
        output=$(agent_cli "$AGENT_C" status 2>/dev/null || true)
        if [[ -z $output ]] || ! python3 -c '
import json
import sys
status = json.load(sys.stdin)["status"]
raise SystemExit(0 if status["controller_connected"] and status["network_active"] else 1)
' <<<"$output"; then
            rejected=1
            break
        fi
        sleep 1
    done
    (( rejected == 1 ))
    docker rm -f "$AGENT_C" >/dev/null
    start_agent "$AGENT_C" "$AGENT_C_ROOT" "$AGENT_C_NETNS"
    for _attempt in $(seq 1 30); do
        output=$(agent_cli "$AGENT_C" status 2>/dev/null || true)
        if [[ -n $output ]] && python3 -c '
import json
import sys
status = json.load(sys.stdin)["status"]
raise SystemExit(0 if status["controller_connected"] and status["network_active"] else 1)
' <<<"$output"; then
            printf 'revoked Agent became ready after restart\n' >&2
            return 1
        fi
        sleep 1
    done
    docker rm -f "$AGENT_C" >/dev/null
}

fault_network_loss_latency() {
    local pid device successes=0
    pid=$(docker inspect "$AGENT_A" --format '{{.State.Pid}}')
    device=$(nsenter -t "$pid" -n ip -o route get "$CONTROLLER_IP" |
        awk '{for (index=1; index<=NF; index++) if ($index == "dev") {print $(index+1); exit}}')
    [[ -n $device ]]
    nsenter -t "$pid" -n tc qdisc replace dev "$device" root netem loss 20% delay 100ms 20ms
    nsenter -t "$pid" -n tc -s qdisc show dev "$device" >"$EVIDENCE_DIR/network-fault-active.txt"
    for _attempt in $(seq 1 10); do
        if ping_agent "$AGENT_A" "$VIRTUAL_IP_B"; then
            ((successes += 1))
        fi
    done
    (( successes >= 3 ))
    nsenter -t "$pid" -n tc qdisc del dev "$device" root
    nsenter -t "$pid" -n tc qdisc show dev "$device" >"$EVIDENCE_DIR/network-fault-recovered.txt"
    for _attempt in $(seq 1 5); do
        ping_agent "$AGENT_A" "$VIRTUAL_IP_B"
    done
}

wait_for_route_suggestion() {
    for _attempt in $(seq 1 360); do
        admin_request GET "/v1/admin/networks/$NETWORK_ID/subnet-route-suggestions" \
            "$TEMPORARY/route-suggestions.json"
        if python3 - "$TEMPORARY/route-suggestions.json" "$LAN_SUBNET" \
            "$TEMPORARY/route-selection.txt" <<'PY'
import json
import sys
from pathlib import Path

for gateway in json.load(open(sys.argv[1], encoding="utf-8")):
    for suggestion in gateway["suggestions"]:
        if suggestion["prefix"] == sys.argv[2]:
            Path(sys.argv[3]).write_text(
                gateway["gateway_node_id_base64"] + "\n" + suggestion["interface_name"] + "\n",
                encoding="utf-8",
            )
            raise SystemExit(0)
raise SystemExit(1)
PY
        then
            return
        fi
        sleep 1
    done
    printf 'LAN subnet suggestion did not appear: %s\n' "$LAN_SUBNET" >&2
    return 1
}

route_present() {
    local expected=$1 output
    output=$(agent_cli "$AGENT_A" routes)
    python3 -c '
import json
import sys
routes = json.load(sys.stdin)["routes"]
raise SystemExit(0 if any(route["prefix"] == sys.argv[1] for route in routes) else 1)
' "$expected" <<<"$output"
}

fault_subnet_route_update() {
    wait_for_route_suggestion
    local gateway interface version
    gateway=$(sed -n '1p' "$TEMPORARY/route-selection.txt")
    interface=$(sed -n '2p' "$TEMPORARY/route-selection.txt")
    version=$(agent_cli "$AGENT_A" status | python3 -c 'import json,sys; print(json.load(sys.stdin)["status"]["configuration_version"])')
    python3 - "$TEMPORARY/route-enable.json" "$version" "$gateway" "$LAN_SUBNET" "$interface" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "expected_configuration_version": int(sys.argv[2]),
    "routes": [{
        "route_id": "gate22-lan",
        "gateway_node_id_base64": sys.argv[3],
        "prefix": sys.argv[4],
        "interface_name": sys.argv[5],
        "mode": "nat",
        "priority": 100,
        "enabled": True,
    }],
}), encoding="utf-8")
PY
    admin_request PUT "/v1/admin/networks/$NETWORK_ID/subnet-routes" \
        "$TEMPORARY/route-enable-response.json" "$TEMPORARY/route-enable.json"
    local enabled_version
    enabled_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["configuration_version"])' \
        "$TEMPORARY/route-enable-response.json")
    wait_configuration_version "$AGENT_A" "$enabled_version"
    wait_configuration_version "$AGENT_B" "$enabled_version"
    for _attempt in $(seq 1 120); do
        route_present "$LAN_SUBNET" && break
        sleep 1
    done
    route_present "$LAN_SUBNET"
    local agent_a_pid
    agent_a_pid=$(docker inspect "$AGENT_A" --format '{{.State.Pid}}')
    nsenter -t "$agent_a_pid" -n ping -c 3 -W 3 "$LAN_TARGET_IP" >"$EVIDENCE_DIR/subnet-route-ping.txt"
    printf '{"expected_configuration_version":%s,"routes":[]}\n' "$enabled_version" \
        >"$TEMPORARY/route-disable.json"
    admin_request PUT "/v1/admin/networks/$NETWORK_ID/subnet-routes" \
        "$TEMPORARY/route-disable-response.json" "$TEMPORARY/route-disable.json"
    local disabled_version
    disabled_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["configuration_version"])' \
        "$TEMPORARY/route-disable-response.json")
    wait_configuration_version "$AGENT_A" "$disabled_version"
    wait_configuration_version "$AGENT_B" "$disabled_version"
    for _attempt in $(seq 1 120); do
        if ! route_present "$LAN_SUBNET"; then
            break
        fi
        sleep 1
    done
    if route_present "$LAN_SUBNET"; then
        printf 'subnet route remained after removal\n' >&2
        return 1
    fi
    if nsenter -t "$agent_a_pid" -n ping -c 1 -W 1 "$LAN_TARGET_IP" >/dev/null 2>&1; then
        printf 'LAN target remained reachable after route removal\n' >&2
        return 1
    fi
}

run_event() {
    local event=$1 function=$2 start end status
    start=$(date +%s%3N)
    set +e
    (set -Eeuo pipefail; "$function")
    status=$?
    set -e
    end=$(date +%s%3N)
    if (( status == 0 )); then
        printf '%s\tPASS\t%s\t%s\n' "$event" "$((end - start))" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            >>"$EVIDENCE_DIR/events.tsv"
    else
        printf '%s\tFAIL\t%s\t%s\n' "$event" "$((end - start))" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
            >>"$EVIDENCE_DIR/events.tsv"
        return "$status"
    fi
}

select_subnets
select_ports
for image in "$AGENT_BUILDER_IMAGE" "$AGENT_RUNTIME_IMAGE" "$ALPINE_IMAGE" "$POSTGRES_IMAGE"; do
    docker image inspect "$image" >/dev/null 2>&1 || docker pull "$image" >/dev/null
done
docker network create --label "com.xs-nexus.gate22.prime=$PROJECT" "$PRIME_NETWORK" >/dev/null
docker run --rm \
    --label "com.xs-nexus.gate22.prime=$PROJECT" \
    --network "$PRIME_NETWORK" \
    --publish "127.0.0.1:$CONTROLLER_PORT:8080" \
    --user 65534:65534 --read-only --cap-drop ALL \
    --security-opt no-new-privileges:true \
    "$ALPINE_IMAGE" true
docker network rm "$PRIME_NETWORK" >/dev/null
capture_host_baseline "$EVIDENCE_DIR/host-before"
docker network create --label "com.xs-nexus.gate22.project=$PROJECT" --subnet "$UNDERLAY_SUBNET" "$NETWORK" >/dev/null
docker network create --label "com.xs-nexus.gate22.project=$PROJECT" --subnet "$LAN_SUBNET" "$LAN_NETWORK" >/dev/null

cd "$ROOT_DIR"
git -c safe.directory="$ROOT_DIR" diff --quiet
git -c safe.directory="$ROOT_DIR" diff --cached --quiet
revision=$(git -c safe.directory="$ROOT_DIR" rev-parse HEAD)
printf '%s\n' "$revision" >"$EVIDENCE_DIR/revision.txt"
printf '%s\n' "$DURATION_SECONDS" >"$EVIDENCE_DIR/duration-seconds.txt"
printf '%s\n' "$SAMPLE_INTERVAL_SECONDS" >"$EVIDENCE_DIR/sample-interval-seconds.txt"
printf '%s\n' "$CALIBRATION" >"$EVIDENCE_DIR/calibration.txt"
printf '%s\n%s\n' "$UNDERLAY_SUBNET" "$LAN_SUBNET" >"$EVIDENCE_DIR/isolated-subnets.txt"

source_date_epoch=$(git -c safe.directory="$ROOT_DIR" show -s --format=%ct HEAD)
mkdir -p "$AGENT_BUILD_TARGET_DIRECTORY"
docker run --rm \
    --mount "type=bind,src=$ROOT_DIR,dst=/src,readonly" \
    --mount "type=bind,src=$AGENT_BUILD_TARGET_DIRECTORY,dst=/target" \
    --workdir /src \
    --env CARGO_TARGET_DIR=/target \
    --env "XS_BUILD_GIT_COMMIT=$revision" \
    --env "XS_BUILD_DATE_EPOCH=$source_date_epoch" \
    "$AGENT_BUILDER_IMAGE" \
    sh -euc '
        cargo build --locked --release -p xs-agent --bin xs-agent -p xs-cli --bin xs
        cargo build --locked --release -p xs-protocol --example derive_ed25519_public
    ' >"$EVIDENCE_DIR/agent-build.log" 2>&1

{
    for image in "$AGENT_BUILDER_IMAGE" "$AGENT_RUNTIME_IMAGE" "$ALPINE_IMAGE" "$POSTGRES_IMAGE"; do
        docker image inspect "$image" --format '{{.Id}} {{json .RepoDigests}}'
    done
} >"$EVIDENCE_DIR/harness-images.txt"

mkdir -p "$BINARY_DIRECTORY"
chmod 0755 "$BINARY_DIRECTORY"
cp "$AGENT_BUILD_TARGET_DIRECTORY/release/xs-agent" "$BINARY_DIRECTORY/xs-agent"
cp "$AGENT_BUILD_TARGET_DIRECTORY/release/xs" "$BINARY_DIRECTORY/xs"
cp "$AGENT_BUILD_TARGET_DIRECTORY/release/examples/derive_ed25519_public" \
    "$BINARY_DIRECTORY/derive_ed25519_public"
chmod 0555 "$BINARY_DIRECTORY/xs-agent" "$BINARY_DIRECTORY/xs" \
    "$BINARY_DIRECTORY/derive_ed25519_public"

{
    docker run --rm \
        --mount "type=bind,src=$BINARY_DIRECTORY,dst=/workspace,readonly" \
        --entrypoint /workspace/xs-agent "$AGENT_RUNTIME_IMAGE" --version
    docker run --rm \
        --mount "type=bind,src=$BINARY_DIRECTORY,dst=/workspace,readonly" \
        --entrypoint /workspace/xs "$AGENT_RUNTIME_IMAGE" --version
} >"$EVIDENCE_DIR/agent-runtime-compatibility.log" 2>&1

prepare_secrets
write_environment
sha256sum "$ENVIRONMENT_FILE" >"$EVIDENCE_DIR/compose-environment.sha256"
compose config --images >"$EVIDENCE_DIR/configured-images.txt"
compose config --services >"$EVIDENCE_DIR/configured-services.txt"
compose build controller relay console db-tools >"$EVIDENCE_DIR/image-build.log" 2>&1
initialize_database
start_application
create_network_and_acl
prepare_agents

compose ps --format json >"$EVIDENCE_DIR/containers-start.json"
for service in controller relay console postgres; do
    container=$(compose ps -q "$service")
    docker inspect "$container" --format '{{.Name}} {{.Image}} {{index .Config.Labels "org.opencontainers.image.revision"}}' \
        >>"$EVIDENCE_DIR/runtime-identities.txt"
done
docker image inspect "$AGENT_RUNTIME_IMAGE" --format '{{.Id}} {{json .RepoDigests}}' \
    >"$EVIDENCE_DIR/agent-runtime-image.txt"
sha256sum "$BINARY_DIRECTORY/xs-agent" "$BINARY_DIRECTORY/xs" >"$EVIDENCE_DIR/agent-binaries.sha256"

printf 'timestamp_utc,epoch,service,container_id,root_pid,rss_kib,threads,fds,cpu_ticks,log_bytes,restart_count,route_count,rule_count,interface_count,db_connections,path_kind,tx_packets_total,rx_packets_total\n' \
    >"$EVIDENCE_DIR/resources.csv"
printf 'event\tstatus\trecovery_milliseconds\tcompleted_at_utc\n' >"$EVIDENCE_DIR/events.tsv"
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$EVIDENCE_DIR/start-utc.txt"

event_names=(
    controller_restart relay_restart postgres_restart agent_restart
    configuration_update enrollment_revocation network_loss_latency subnet_route_update
)
event_functions=(
    fault_controller_restart fault_relay_restart fault_postgres_restart fault_agent_restart
    fault_configuration_update fault_enrollment_revocation fault_network_loss_latency fault_subnet_route_update
)
event_percentages=(10 20 30 40 50 60 70 80)
start_epoch=$(date +%s)
deadline=$((start_epoch + DURATION_SECONDS))
event_index=0
while (( $(date +%s) < deadline )); do
    sample_all
    if (( event_index < ${#event_names[@]} )); then
        target=$((start_epoch + DURATION_SECONDS * event_percentages[event_index] / 100))
        if (( $(date +%s) >= target )); then
            run_event "${event_names[event_index]}" "${event_functions[event_index]}"
            ((event_index += 1))
        fi
    fi
    sleep "$SAMPLE_INTERVAL_SECONDS"
done
while (( event_index < ${#event_names[@]} )); do
    run_event "${event_names[event_index]}" "${event_functions[event_index]}"
    ((event_index += 1))
done
sample_all
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$EVIDENCE_DIR/end-utc.txt"

expected_samples=$((DURATION_SECONDS / SAMPLE_INTERVAL_SECONDS))
minimum_samples=$((expected_samples * 2 / 3))
(( minimum_samples >= 2 )) || minimum_samples=2
python3 "$SUMMARY_SCRIPT" "$EVIDENCE_DIR/resources.csv" "$EVIDENCE_DIR/events.tsv" \
    "$minimum_samples" "$EVIDENCE_DIR/summary.json"
compose ps --format json >"$EVIDENCE_DIR/containers-end.json"
record_logs
remove_resources
capture_host_baseline "$EVIDENCE_DIR/host-after"
for suffix in docker-networks.txt docker-volumes.txt docker-containers.txt default-routes.json rules.json links.txt nftables.json failed-services.txt onepanel-network.txt; do
    cmp "$EVIDENCE_DIR/host-before-$suffix" "$EVIDENCE_DIR/host-after-$suffix"
done
finalize_evidence 0
trap - EXIT INT TERM
rm -rf "$TEMPORARY"
printf 'current-revision soak verification passed; evidence: %s\n' "$EVIDENCE_DIR"
