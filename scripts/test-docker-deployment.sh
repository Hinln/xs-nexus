#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STACK="$ROOT_DIR/deploy/docker/xs-nexus-stack.sh"
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"
EXTERNAL_ENVIRONMENT=${XS_TEST_EXTERNAL_ENVIRONMENT:-/etc/xs-nexus/controller.env}
TEMPORARY=$(mktemp -d)
ENVIRONMENT_FILE="$TEMPORARY/dev.compose.env"
BAD_DATABASE_ENVIRONMENT="$TEMPORARY/bad-database.compose.env"
BAD_IMAGE_ENVIRONMENT="$TEMPORARY/bad-image.compose.env"
BAD_HTTP_BIND_ENVIRONMENT="$TEMPORARY/bad-http-bind.compose.env"
BAD_UDP_BIND_ENVIRONMENT="$TEMPORARY/bad-udp-bind.compose.env"
BAD_PORT_ENVIRONMENT="$TEMPORARY/bad-port.compose.env"
RC_ENVIRONMENT="$TEMPORARY/rc.compose.env"
BAD_RC_IMAGE_ENVIRONMENT="$TEMPORARY/bad-rc-image.compose.env"
CONTROLLER_SECRETS="$TEMPORARY/controller"
BAD_CONTROLLER_SECRETS="$TEMPORARY/bad-controller"
RELEASE_DIRECTORY="$TEMPORARY/releases/linux/stable"
WINDOWS_RELEASE_DIRECTORY="$TEMPORARY/releases/windows/stable"
RELAY_SECRETS="$TEMPORARY/relay"
BACKUP_DIRECTORY="$TEMPORARY/backups"
REPLICA_DIRECTORY=$(mktemp -d /dev/shm/xs-m52-replica.XXXXXX)
STATE_DIRECTORY="$TEMPORARY/state"
TEST_DATABASE_SCHEMA=xs_nexus_m52_deploy_dev
CONTROLLER_PORT=38180
CONSOLE_PORT=38181
DISCOVERY_PORT=42180
RELAY_PORT=42181
ADMIN_TOKEN=''
DATABASE_URL_VALUE=''
HOST_DATABASE_URL_VALUE=''
NETWORK_MEMBERS_BEFORE=''
DOCKER_NETWORKS_BEFORE=''
DEFAULT_ROUTES_BEFORE=''
FIREWALL_BEFORE=''
LOCAL_RETENTION_DAYS=30
REPLICA_RETENTION_DAYS=180
MIN_RETAINED_BACKUPS=3
OCCUPIED_PORT=38182
OCCUPIED_PID=''

compose() {
    docker compose --env-file "$ENVIRONMENT_FILE" -f "$COMPOSE_FILE" "$@"
}

snapshot_firewall() {
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

reset_schema() {
    [[ -n $HOST_DATABASE_URL_VALUE ]] || return
    DATABASE_URL=$HOST_DATABASE_URL_VALUE DATABASE_SCHEMA=$TEST_DATABASE_SCHEMA \
        cargo run --quiet -p xs-controller --example reset_test_schema >/dev/null
}

cleanup() {
    local status=$?
    if [[ -n $OCCUPIED_PID ]]; then
        kill "$OCCUPIED_PID" >/dev/null 2>&1 || true
        wait "$OCCUPIED_PID" 2>/dev/null || true
    fi
    if [[ -f $ENVIRONMENT_FILE ]]; then
        "$STACK" --env-file "$ENVIRONMENT_FILE" down >/dev/null 2>&1 || true
    fi
    reset_schema || true
    rm -rf -- "$TEMPORARY"
    if [[ $REPLICA_DIRECTORY == /dev/shm/xs-m52-replica.* && -d $REPLICA_DIRECTORY ]]; then
        rm -rf -- "$REPLICA_DIRECTORY"
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

wait_for_http() {
    local url=$1
    for _attempt in $(seq 1 30); do
        if curl --fail --silent --show-error "$url" >/dev/null 2>&1; then
            return
        fi
        sleep 1
    done
    return 1
}

write_environment() {
    local path=$1 controller_secrets=$2 controller_image=$3 database_schema=$4
    cat >"$path" <<EOF
XS_DEPLOYMENT=dev
XS_COMPOSE_PROJECT_NAME=xs-nexus-dev
XS_RELEASE_REVISION=$(git rev-parse HEAD)
XS_CONTROLLER_IMAGE=$controller_image
XS_MIGRATION_IMAGE=xs-nexus/controller:m52test
XS_RELAY_IMAGE=xs-nexus/relay:m52test
XS_CONSOLE_IMAGE=xs-nexus/console:m52test
XS_DB_TOOLS_IMAGE=xs-nexus/db-tools:m52test
XS_CONTROLLER_SECRETS_DIR=$controller_secrets
XS_LINUX_RELEASE_DIR=$RELEASE_DIRECTORY
XS_WINDOWS_RELEASE_DIR=$WINDOWS_RELEASE_DIRECTORY
XS_RELAY_SECRETS_DIR=$RELAY_SECRETS
XS_BACKUP_DIR=$BACKUP_DIRECTORY
XS_BACKUP_REPLICA_DIR=$REPLICA_DIRECTORY
XS_BACKUP_LOCAL_RETENTION_DAYS=$LOCAL_RETENTION_DAYS
XS_BACKUP_REPLICA_RETENTION_DAYS=$REPLICA_RETENTION_DAYS
XS_BACKUP_MIN_RETAINED=$MIN_RETAINED_BACKUPS
XS_STATE_DIR=$STATE_DIRECTORY
XS_DATABASE_SCHEMA=$database_schema
XS_BIND_ADDRESS=127.0.0.1
XS_UDP_BIND_ADDRESS=127.0.0.1
XS_CONTROLLER_HTTP_PORT=$CONTROLLER_PORT
XS_CONSOLE_HTTP_PORT=$CONSOLE_PORT
XS_DISCOVERY_UDP_PORT=$DISCOVERY_PORT
XS_RELAY_UDP_PORT=$RELAY_PORT
XS_DISCOVERY_PUBLIC_ENDPOINT=127.0.0.1:$DISCOVERY_PORT
XS_RELAY_ID_BASE64=$RELAY_ID_BASE64
XS_CONSOLE_COOKIE_SECURE=false
XS_RUST_LOG=info
EOF
    chmod 0600 "$path"
}

create_network() {
    local name=$1 address_pool=$2
    curl --fail --silent --show-error \
        -X POST "http://127.0.0.1:$CONTROLLER_PORT/v1/admin/networks" \
        -H "Authorization: Bearer $ADMIN_TOKEN" \
        -H 'Content-Type: application/json' \
        --data "{\"name\":\"$name\",\"address_pool\":\"$address_pool\",\"reserved_addresses\":16}"
}

list_networks() {
    curl --fail --silent --show-error \
        "http://127.0.0.1:$CONTROLLER_PORT/v1/admin/networks" \
        -H "Authorization: Bearer $ADMIN_TOKEN"
}

assert_service_security() {
    local service=$1 container user read_only cap_drop security_options
    container=$(compose ps -q "$service")
    [[ -n $container ]]
    user=$(docker inspect "$container" --format '{{.Config.User}}')
    read_only=$(docker inspect "$container" --format '{{.HostConfig.ReadonlyRootfs}}')
    cap_drop=$(docker inspect "$container" --format '{{json .HostConfig.CapDrop}}')
    security_options=$(docker inspect "$container" --format '{{json .HostConfig.SecurityOpt}}')
    [[ $user != 0 && $user != root && $user != 0:* ]]
    [[ $read_only == true ]]
    grep -F 'ALL' <<<"$cap_drop" >/dev/null
    grep -F 'no-new-privileges' <<<"$security_options" >/dev/null
}

cd "$ROOT_DIR"
[[ -r $EXTERNAL_ENVIRONMENT ]]
for port in "$CONTROLLER_PORT" "$CONSOLE_PORT" "$DISCOVERY_PORT" "$RELAY_PORT"; do
    if ss -H -lntup | grep -Eq "[:.]$port([[:space:]]|$)"; then
        printf 'required test port is already in use: %s\n' "$port" >&2
        exit 1
    fi
done

DOCKER_NETWORKS_BEFORE=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
NETWORK_MEMBERS_BEFORE=$(docker network inspect 1panel-network --format '{{json .Containers}}')
DEFAULT_ROUTES_BEFORE=$(ip -json route show default)
FIREWALL_BEFORE=$(snapshot_firewall)

set -a
# shellcheck disable=SC1090
source "$EXTERNAL_ENVIRONMENT"
set +a
DATABASE_URL_VALUE=${DATABASE_URL:?DATABASE_URL is required}
if [[ -n ${HOST_DATABASE_URL:-} ]]; then
    HOST_DATABASE_URL_VALUE=$HOST_DATABASE_URL
else
    HOST_DATABASE_URL_VALUE=$(python3 -c '
import sys
from urllib.parse import urlsplit, urlunsplit

parsed = urlsplit(sys.stdin.read())
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)))
' <<<"$DATABASE_URL_VALUE")
fi
unset DATABASE_URL HOST_DATABASE_URL DATABASE_SCHEMA REDIS_URL MYSQL_URL

mkdir -p "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$STATE_DIRECTORY" "$RELEASE_DIRECTORY" "$WINDOWS_RELEASE_DIRECTORY"
chown 65532:65532 "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY"
chmod 0700 "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY" "$STATE_DIRECTORY"
chmod 0755 "$RELEASE_DIRECTORY"
chmod 0755 "$WINDOWS_RELEASE_DIRECTORY"

ADMIN_TOKEN=$(openssl rand -hex 32)
CONSOLE_PASSWORD=$(openssl rand -base64 24 | tr -d '\n')
printf '%s' "$DATABASE_URL_VALUE" >"$CONTROLLER_SECRETS/database-url"
printf '%s' "$ADMIN_TOKEN" >"$CONTROLLER_SECRETS/admin-api-token"
printf '%s' "$CONSOLE_PASSWORD" >"$CONTROLLER_SECRETS/console-bootstrap-password"
head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/credential-signing-key"
head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/configuration-signing-key"
printf '%s\n' 'age1wcd4cep4z26php4gteja7n2wgxyukpe4nc5xn8dr6zk85gg26chsy9qyj8' >"$CONTROLLER_SECRETS/backup-recipient"
printf '%s\n' 'test-public-key' >"$RELEASE_DIRECTORY/release-public-key.pem"
for architecture in x86_64 aarch64; do
    target="$architecture-unknown-linux-gnu"
    printf '%s\n' 'test-manifest' >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.manifest"
    head -c 64 /dev/urandom >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.manifest.sig"
    printf '%s\n' 'test-archive' >"$RELEASE_DIRECTORY/xs-nexus-0.1.0-$target.tar.gz"
done
chmod 0644 "$RELEASE_DIRECTORY"/*
printf '%s\n' 'Write-Output xs-nexus-test' >"$WINDOWS_RELEASE_DIRECTORY/install.ps1"
printf '%s\n' '{"schema_version":1}' >"$WINDOWS_RELEASE_DIRECTORY/xs-nexus-0.1.0-x86_64-pc-windows-msvc.manifest.json"
printf '%s\n' 'test-archive' >"$WINDOWS_RELEASE_DIRECTORY/xs-nexus-0.1.0-x86_64-pc-windows-msvc.zip"
chmod 0644 "$WINDOWS_RELEASE_DIRECTORY"/*
head -c 32 /dev/urandom >"$RELAY_SECRETS/identity-key"
{
    printf 'format=xs-nexus-replica-v1\n'
    printf 'deployment=dev\n'
    printf 'target_id=m52-distinct-filesystem-test\n'
} >"$REPLICA_DIRECTORY/.xs-nexus-replica"

cargo run --quiet -p xs-protocol --example derive_ed25519_public -- \
    "$CONTROLLER_SECRETS/credential-signing-key" "$RELAY_SECRETS/controller-credential-public-key"
cargo run --quiet -p xs-protocol --example derive_ed25519_public -- \
    "$CONTROLLER_SECRETS/configuration-signing-key" "$CONTROLLER_SECRETS/update-signing-public-key"
cargo run --quiet -p xs-protocol --example derive_ed25519_public -- \
    "$RELAY_SECRETS/identity-key" "$TEMPORARY/relay-public-key"

RELAY_ID_BASE64=$(python3 -c 'import base64, os; print(base64.urlsafe_b64encode(os.urandom(16)).rstrip(b"=").decode())')
python3 - "$CONTROLLER_SECRETS/relay-catalog.json" "$RELAY_ID_BASE64" "$RELAY_PORT" "$TEMPORARY/relay-public-key" <<'PY'
import base64
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

catalog = [{
    "relay_id_base64": sys.argv[2],
    "endpoint": f"127.0.0.1:{sys.argv[3]}",
    "identity_public_key_base64": base64.urlsafe_b64encode(
        Path(sys.argv[4]).read_bytes()
    ).rstrip(b"=").decode(),
    "priority": 100,
    "expires_at": (datetime.now(timezone.utc) + timedelta(days=1)).isoformat().replace("+00:00", "Z"),
}]
Path(sys.argv[1]).write_text(json.dumps(catalog), encoding="utf-8")
PY
rm -f -- "$TEMPORARY/relay-public-key"
chown -R 65532:65532 "$CONTROLLER_SECRETS" "$RELAY_SECRETS"
chown 65532:65532 "$REPLICA_DIRECTORY/.xs-nexus-replica"
find "$CONTROLLER_SECRETS" "$RELAY_SECRETS" -type f -exec chmod 0400 {} +
chmod 0600 "$REPLICA_DIRECTORY/.xs-nexus-replica"

cp -a "$CONTROLLER_SECRETS/." "$BAD_CONTROLLER_SECRETS/"
printf '%s' 'postgresql://127.0.0.1:1/invalid' >"$BAD_CONTROLLER_SECRETS/database-url"
chown -R 65532:65532 "$BAD_CONTROLLER_SECRETS"
find "$BAD_CONTROLLER_SECRETS" -type f -exec chmod 0400 {} +

write_environment "$ENVIRONMENT_FILE" "$CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_DATABASE_ENVIRONMENT" "$BAD_CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_IMAGE_ENVIRONMENT" "$CONTROLLER_SECRETS" alpine:3.22 "$TEST_DATABASE_SCHEMA"
cp -- "$ENVIRONMENT_FILE" "$RC_ENVIRONMENT"
revision=$(git rev-parse HEAD)
sed -i \
    -e 's/^XS_DEPLOYMENT=.*/XS_DEPLOYMENT=rc/' \
    -e 's/^XS_COMPOSE_PROJECT_NAME=.*/XS_COMPOSE_PROJECT_NAME=xs-nexus-rc/' \
    -e "s/^XS_CONTROLLER_IMAGE=.*/XS_CONTROLLER_IMAGE=xs-nexus\/controller:$revision/" \
    -e "s/^XS_MIGRATION_IMAGE=.*/XS_MIGRATION_IMAGE=xs-nexus\/controller:$revision/" \
    -e "s/^XS_RELAY_IMAGE=.*/XS_RELAY_IMAGE=xs-nexus\/relay:$revision/" \
    -e "s/^XS_CONSOLE_IMAGE=.*/XS_CONSOLE_IMAGE=xs-nexus\/console:$revision/" \
    -e "s/^XS_DB_TOOLS_IMAGE=.*/XS_DB_TOOLS_IMAGE=xs-nexus\/db-tools:$revision/" \
    -e 's/^XS_DATABASE_SCHEMA=.*/XS_DATABASE_SCHEMA=xs_nexus_m52_deploy_rc/' \
    "$RC_ENVIRONMENT"
cp -- "$RC_ENVIRONMENT" "$BAD_RC_IMAGE_ENVIRONMENT"
sed -i 's/^XS_CONTROLLER_IMAGE=.*/XS_CONTROLLER_IMAGE=xs-nexus\/controller:stale-revision/' \
    "$BAD_RC_IMAGE_ENVIRONMENT"
cp -- "$ENVIRONMENT_FILE" "$BAD_HTTP_BIND_ENVIRONMENT"
sed -i 's/^XS_BIND_ADDRESS=.*/XS_BIND_ADDRESS=0.0.0.0/' "$BAD_HTTP_BIND_ENVIRONMENT"
cp -- "$ENVIRONMENT_FILE" "$BAD_UDP_BIND_ENVIRONMENT"
sed -i 's/^XS_UDP_BIND_ADDRESS=.*/XS_UDP_BIND_ADDRESS=203.0.113.1/' "$BAD_UDP_BIND_ENVIRONMENT"
chmod 0600 "$BAD_HTTP_BIND_ENVIRONMENT" "$BAD_UDP_BIND_ENVIRONMENT" \
    "$RC_ENVIRONMENT" "$BAD_RC_IMAGE_ENVIRONMENT"
sed -i 's/^deployment=dev$/deployment=rc/' "$REPLICA_DIRECTORY/.xs-nexus-replica"
chown 65532:65532 "$REPLICA_DIRECTORY/.xs-nexus-replica"
"$STACK" --env-file "$RC_ENVIRONMENT" preflight
if "$STACK" --env-file "$BAD_RC_IMAGE_ENVIRONMENT" preflight >/dev/null 2>&1; then
    printf 'RC image tag unrelated to the release revision unexpectedly passed preflight\n' >&2
    exit 1
fi
sed -i 's/^deployment=rc$/deployment=dev/' "$REPLICA_DIRECTORY/.xs-nexus-replica"
chown 65532:65532 "$REPLICA_DIRECTORY/.xs-nexus-replica"
if "$STACK" --env-file "$BAD_HTTP_BIND_ENVIRONMENT" preflight >/dev/null 2>&1; then
    printf 'public plaintext HTTP bind unexpectedly passed preflight\n' >&2
    exit 1
fi
if "$STACK" --env-file "$BAD_UDP_BIND_ENVIRONMENT" preflight >/dev/null 2>&1; then
    printf 'unapproved UDP bind unexpectedly passed preflight\n' >&2
    exit 1
fi
cp -- "$ENVIRONMENT_FILE" "$BAD_PORT_ENVIRONMENT"
sed -i "s/^XS_CONTROLLER_HTTP_PORT=.*/XS_CONTROLLER_HTTP_PORT=$OCCUPIED_PORT/" "$BAD_PORT_ENVIRONMENT"
chmod 0600 "$BAD_PORT_ENVIRONMENT"
python3 -m http.server "$OCCUPIED_PORT" --bind 127.0.0.1 >/dev/null 2>&1 &
OCCUPIED_PID=$!
for _attempt in $(seq 1 20); do
    ss -H -ltn "sport = :$OCCUPIED_PORT" | grep -q . && break
    sleep 0.1
done
if "$STACK" --env-file "$BAD_PORT_ENVIRONMENT" preflight >/dev/null 2>&1; then
    printf 'occupied Controller HTTP port unexpectedly passed preflight\n' >&2
    exit 1
fi
kill "$OCCUPIED_PID"
wait "$OCCUPIED_PID" 2>/dev/null || true
OCCUPIED_PID=''

reset_schema
"$STACK" --env-file "$ENVIRONMENT_FILE" preflight
"$STACK" --env-file "$ENVIRONMENT_FILE" build
BACKUP_KEY_DIRECTORY="$TEMPORARY/backup-key"
mkdir -m 0700 "$BACKUP_KEY_DIRECTORY"
chown 65532:65532 "$BACKUP_KEY_DIRECTORY"
docker run --rm --user 65532:65532 \
    --volume "$BACKUP_KEY_DIRECTORY:/keys" \
    --entrypoint /bin/sh xs-nexus/db-tools:m52test \
    -c 'age-keygen -o /keys/identity 2>/keys/keygen.log'
BACKUP_RECIPIENT=$(sed -n 's/^Public key: //p' "$BACKUP_KEY_DIRECTORY/keygen.log")
[[ $BACKUP_RECIPIENT =~ ^age1[0-9a-z]{58}$ ]]
printf '%s\n' "$BACKUP_RECIPIENT" >"$CONTROLLER_SECRETS/backup-recipient"
printf '%s\n' "$BACKUP_RECIPIENT" >"$BAD_CONTROLLER_SECRETS/backup-recipient"
chown 65532:65532 "$CONTROLLER_SECRETS/backup-recipient" \
    "$BAD_CONTROLLER_SECRETS/backup-recipient" "$BACKUP_KEY_DIRECTORY/identity"
chmod 0400 "$CONTROLLER_SECRETS/backup-recipient" \
    "$BAD_CONTROLLER_SECRETS/backup-recipient" "$BACKUP_KEY_DIRECTORY/identity"
rm -- "$BACKUP_KEY_DIRECTORY/keygen.log"
"$STACK" --env-file "$ENVIRONMENT_FILE" deploy

wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready"
wait_for_http "http://127.0.0.1:$CONSOLE_PORT/console-health"
curl --fail --silent --show-error "http://127.0.0.1:$CONSOLE_PORT/health/ready" >/dev/null
curl --fail --silent --show-error "http://127.0.0.1:$CONTROLLER_PORT/install" \
    | cmp - "$ROOT_DIR/installers/linux/xs-nexus-one-click.sh"
curl --fail --silent --show-error \
    "http://127.0.0.1:$CONTROLLER_PORT/downloads/linux/stable/release-public-key.pem" \
    | cmp - "$RELEASE_DIRECTORY/release-public-key.pem"
if curl --fail --silent --show-error \
    "http://127.0.0.1:$CONTROLLER_PORT/downloads/linux/stable/unexpected" >/dev/null 2>&1; then
    printf 'unexpected Linux release filename was served\n' >&2
    exit 1
fi
curl --fail --silent --show-error -A 'WindowsPowerShell/5.1' "http://127.0.0.1:$CONTROLLER_PORT/install" \
    | cmp - "$WINDOWS_RELEASE_DIRECTORY/install.ps1"
curl --fail --silent --show-error "http://127.0.0.1:$CONTROLLER_PORT/install/windows" \
    | cmp - "$WINDOWS_RELEASE_DIRECTORY/install.ps1"
curl --fail --silent --show-error \
    "http://127.0.0.1:$CONTROLLER_PORT/downloads/windows/stable/xs-nexus-0.1.0-x86_64-pc-windows-msvc.manifest.json" \
    | cmp - "$WINDOWS_RELEASE_DIRECTORY/xs-nexus-0.1.0-x86_64-pc-windows-msvc.manifest.json"
if curl --fail --silent --show-error \
    "http://127.0.0.1:$CONTROLLER_PORT/downloads/windows/stable/unexpected" >/dev/null 2>&1; then
    printf 'unexpected Windows release filename was served\n' >&2
    exit 1
fi
relay_container=$(compose ps -q relay)
docker exec "$relay_container" /usr/local/bin/xs-relay healthcheck

for service in controller relay console; do
    assert_service_security "$service"
done
[[ $(compose ps --status running -q | wc -l) -eq 3 ]]
if docker ps --format '{{.Ports}}' --filter label=com.docker.compose.project=xs-nexus-dev |
    grep -Eq '(:|->)(3306|5432|6379)(/|-)'; then
    printf 'project container published a database port\n' >&2
    exit 1
fi

first_network=$(create_network m52-before-backup 100.120.52.0/24)
first_id=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$first_network")
"$STACK" --env-file "$ENVIRONMENT_FILE" backup m52-snapshot
"$STACK" --env-file "$ENVIRONMENT_FILE" verify-backup m52-snapshot
"$STACK" --env-file "$ENVIRONMENT_FILE" verify-backup-deep m52-snapshot \
    --identity-file "$BACKUP_KEY_DIRECTORY/identity"
mv -- "$REPLICA_DIRECTORY/m52-snapshot.replication" "$TEMPORARY/m52-snapshot.replication"
if "$STACK" --env-file "$ENVIRONMENT_FILE" prune-backups \
    --confirm-before "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --identity-file "$BACKUP_KEY_DIRECTORY/identity" >/dev/null 2>&1; then
    printf 'retention accepted a replica without its replication receipt\n' >&2
    exit 1
fi
mv -- "$TEMPORARY/m52-snapshot.replication" "$REPLICA_DIRECTORY/m52-snapshot.replication"
[[ -f $BACKUP_DIRECTORY/m52-snapshot.dump.age ]]
[[ -f $BACKUP_DIRECTORY/m52-snapshot.manifest.age ]]
[[ -f $BACKUP_DIRECTORY/m52-snapshot.index ]]
[[ -f $BACKUP_DIRECTORY/m52-snapshot.replicated ]]
[[ -f $REPLICA_DIRECTORY/m52-snapshot.dump.age ]]
[[ -f $REPLICA_DIRECTORY/m52-snapshot.replication ]]
[[ ! -e $BACKUP_DIRECTORY/m52-snapshot.dump ]]
second_network=$(create_network m52-after-backup 100.121.52.0/24)
second_id=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$second_network")

cp "$BACKUP_DIRECTORY/m52-snapshot.dump.age" "$TEMPORARY/original.dump.age"
printf 'x' >>"$BACKUP_DIRECTORY/m52-snapshot.dump.age"
if "$STACK" --env-file "$ENVIRONMENT_FILE" verify-backup m52-snapshot >/dev/null 2>&1; then
    printf 'tampered backup was accepted\n' >&2
    exit 1
fi
mv -- "$TEMPORARY/original.dump.age" "$BACKUP_DIRECTORY/m52-snapshot.dump.age"
chown 65532:65532 "$BACKUP_DIRECTORY/m52-snapshot.dump.age"
chmod 0600 "$BACKUP_DIRECTORY/m52-snapshot.dump.age"

WRONG_KEY_DIRECTORY="$TEMPORARY/wrong-backup-key"
mkdir -m 0700 "$WRONG_KEY_DIRECTORY"
chown 65532:65532 "$WRONG_KEY_DIRECTORY"
docker run --rm --user 65532:65532 \
    --volume "$WRONG_KEY_DIRECTORY:/keys" \
    --entrypoint /bin/sh xs-nexus/db-tools:m52test \
    -c 'age-keygen -o /keys/identity 2>/dev/null'
chmod 0400 "$WRONG_KEY_DIRECTORY/identity"
if "$STACK" --env-file "$ENVIRONMENT_FILE" verify-backup-deep m52-snapshot \
    --identity-file "$WRONG_KEY_DIRECTORY/identity" >/dev/null 2>&1; then
    printf 'backup decrypted with an unrelated identity\n' >&2
    exit 1
fi

mkdir "$TEMPORARY/local-backup-copy"
mv -- "$BACKUP_DIRECTORY"/m52-snapshot.* "$TEMPORARY/local-backup-copy/"
"$STACK" --env-file "$ENVIRONMENT_FILE" fetch-backup m52-snapshot
cmp -s "$BACKUP_DIRECTORY/m52-snapshot.dump.age" \
    "$TEMPORARY/local-backup-copy/m52-snapshot.dump.age"
rm -rf -- "$TEMPORARY/local-backup-copy"

"$STACK" --env-file "$ENVIRONMENT_FILE" restore m52-snapshot \
    --confirm-schema "$TEST_DATABASE_SCHEMA" \
    --identity-file "$BACKUP_KEY_DIRECTORY/identity"
networks_after_restore=$(list_networks)
python3 - "$first_id" "$second_id" "$networks_after_restore" <<'PY'
import json
import sys

networks = {item["id"] for item in json.loads(sys.argv[3])}
assert sys.argv[1] in networks
assert sys.argv[2] not in networks
PY

sleep 1
"$STACK" --env-file "$ENVIRONMENT_FILE" backup m52-retention-newest
sleep 1
LOCAL_RETENTION_DAYS=0
REPLICA_RETENTION_DAYS=3650
MIN_RETAINED_BACKUPS=1
write_environment "$ENVIRONMENT_FILE" "$CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_DATABASE_ENVIRONMENT" "$BAD_CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_IMAGE_ENVIRONMENT" "$CONTROLLER_SECRETS" alpine:3.22 "$TEST_DATABASE_SCHEMA"
RETENTION_CONFIRMATION=$(date -u +%Y-%m-%dT%H:%M:%SZ)
"$STACK" --env-file "$ENVIRONMENT_FILE" prune-backups \
    --confirm-before "$RETENTION_CONFIRMATION" \
    --identity-file "$BACKUP_KEY_DIRECTORY/identity"
[[ ! -e $BACKUP_DIRECTORY/m52-snapshot.index ]]
[[ -e $REPLICA_DIRECTORY/m52-snapshot.index ]]
"$STACK" --env-file "$ENVIRONMENT_FILE" fetch-backup m52-snapshot

sleep 1
REPLICA_RETENTION_DAYS=0
write_environment "$ENVIRONMENT_FILE" "$CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_DATABASE_ENVIRONMENT" "$BAD_CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_IMAGE_ENVIRONMENT" "$CONTROLLER_SECRETS" alpine:3.22 "$TEST_DATABASE_SCHEMA"
RETENTION_CONFIRMATION=$(date -u +%Y-%m-%dT%H:%M:%SZ)
"$STACK" --env-file "$ENVIRONMENT_FILE" prune-backups \
    --confirm-before "$RETENTION_CONFIRMATION" \
    --identity-file "$BACKUP_KEY_DIRECTORY/identity"
[[ -d $REPLICA_DIRECTORY/.destroyed ]]
[[ -n $(find "$REPLICA_DIRECTORY/.destroyed" -maxdepth 1 -type f -name '*.tombstone' -print -quit) ]]
[[ -z $(find "$BACKUP_DIRECTORY" "$REPLICA_DIRECTORY" -maxdepth 1 -type f -name '*.dump' -print -quit) ]]
if "$STACK" --env-file "$ENVIRONMENT_FILE" backup m52-snapshot >/dev/null 2>&1; then
    printf 'destroyed immutable backup name was reused\n' >&2
    exit 1
fi

controller_before_failed_migration=$(compose ps -q controller)
if "$STACK" --env-file "$BAD_DATABASE_ENVIRONMENT" deploy >/dev/null 2>&1; then
    printf 'deployment with failed migration unexpectedly succeeded\n' >&2
    exit 1
fi
[[ $(compose ps -q controller) == "$controller_before_failed_migration" ]]
wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready"

controller_image_before_failed_activation=$(docker inspect "$(compose ps -q controller)" --format '{{.Image}}')
if "$STACK" --env-file "$BAD_IMAGE_ENVIRONMENT" deploy >/dev/null 2>&1; then
    printf 'deployment with failed activation unexpectedly succeeded\n' >&2
    exit 1
fi
controller_image_after_failed_activation=$(docker inspect "$(compose ps -q controller)" --format '{{.Image}}')
[[ $controller_image_after_failed_activation == "$controller_image_before_failed_activation" ]]
wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready"

"$STACK" --env-file "$ENVIRONMENT_FILE" status >/dev/null
if [[ ${XS_STABILITY_DURATION_SECONDS:-0} -gt 0 ]]; then
    XS_COMPOSE_PROJECT_NAME=xs-nexus-dev \
    XS_CONTROLLER_HTTP_PORT=$CONTROLLER_PORT \
    XS_CONSOLE_HTTP_PORT=$CONSOLE_PORT \
        "$ROOT_DIR/scripts/test-runtime-stability.sh" "$ENVIRONMENT_FILE"
fi
"$STACK" --env-file "$ENVIRONMENT_FILE" down
reset_schema

[[ -z $(docker ps -a --filter label=com.docker.compose.project=xs-nexus-dev --format '{{.ID}}') ]]
[[ $(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort) == "$DOCKER_NETWORKS_BEFORE" ]]
[[ $(docker network inspect 1panel-network --format '{{json .Containers}}') == "$NETWORK_MEMBERS_BEFORE" ]]
[[ $(ip -json route show default) == "$DEFAULT_ROUTES_BEFORE" ]]
[[ $(snapshot_firewall) == "$FIREWALL_BEFORE" ]]

printf 'Docker/1Panel deployment lifecycle test passed\n'
