#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STACK="$ROOT_DIR/deploy/docker/xs-nexus-stack.sh"
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"
EXTERNAL_ENVIRONMENT=/etc/xs-nexus/controller.env
TEMPORARY=$(mktemp -d)
ENVIRONMENT_FILE="$TEMPORARY/dev.compose.env"
BAD_DATABASE_ENVIRONMENT="$TEMPORARY/bad-database.compose.env"
BAD_IMAGE_ENVIRONMENT="$TEMPORARY/bad-image.compose.env"
CONTROLLER_SECRETS="$TEMPORARY/controller"
BAD_CONTROLLER_SECRETS="$TEMPORARY/bad-controller"
RELAY_SECRETS="$TEMPORARY/relay"
BACKUP_DIRECTORY="$TEMPORARY/backups"
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
    if [[ -f $ENVIRONMENT_FILE ]]; then
        "$STACK" --env-file "$ENVIRONMENT_FILE" down >/dev/null 2>&1 || true
    fi
    reset_schema || true
    rm -rf -- "$TEMPORARY"
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
XS_RELAY_SECRETS_DIR=$RELAY_SECRETS
XS_BACKUP_DIR=$BACKUP_DIRECTORY
XS_STATE_DIR=$STATE_DIRECTORY
XS_DATABASE_SCHEMA=$database_schema
XS_BIND_ADDRESS=127.0.0.1
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
HOST_DATABASE_URL_VALUE=$(python3 -c '
import sys
from urllib.parse import urlsplit, urlunsplit

parsed = urlsplit(sys.stdin.read())
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)))
' <<<"$DATABASE_URL_VALUE")
unset DATABASE_URL DATABASE_SCHEMA REDIS_URL MYSQL_URL

mkdir -p "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$STATE_DIRECTORY"
chown 65532:65532 "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY"
chmod 0700 "$CONTROLLER_SECRETS" "$BAD_CONTROLLER_SECRETS" "$RELAY_SECRETS" "$BACKUP_DIRECTORY" "$STATE_DIRECTORY"

ADMIN_TOKEN=$(openssl rand -hex 32)
CONSOLE_PASSWORD=$(openssl rand -base64 24 | tr -d '\n')
printf '%s' "$DATABASE_URL_VALUE" >"$CONTROLLER_SECRETS/database-url"
printf '%s' "$ADMIN_TOKEN" >"$CONTROLLER_SECRETS/admin-api-token"
printf '%s' "$CONSOLE_PASSWORD" >"$CONTROLLER_SECRETS/console-bootstrap-password"
head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/credential-signing-key"
head -c 32 /dev/urandom >"$CONTROLLER_SECRETS/configuration-signing-key"
head -c 32 /dev/urandom >"$RELAY_SECRETS/identity-key"

cargo run --quiet -p xs-protocol --example derive_ed25519_public -- \
    "$CONTROLLER_SECRETS/credential-signing-key" "$RELAY_SECRETS/controller-credential-public-key"
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
find "$CONTROLLER_SECRETS" "$RELAY_SECRETS" -type f -exec chmod 0400 {} +

cp -a "$CONTROLLER_SECRETS/." "$BAD_CONTROLLER_SECRETS/"
printf '%s' 'postgresql://127.0.0.1:1/invalid' >"$BAD_CONTROLLER_SECRETS/database-url"
chown -R 65532:65532 "$BAD_CONTROLLER_SECRETS"
find "$BAD_CONTROLLER_SECRETS" -type f -exec chmod 0400 {} +

write_environment "$ENVIRONMENT_FILE" "$CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_DATABASE_ENVIRONMENT" "$BAD_CONTROLLER_SECRETS" xs-nexus/controller:m52test "$TEST_DATABASE_SCHEMA"
write_environment "$BAD_IMAGE_ENVIRONMENT" "$CONTROLLER_SECRETS" alpine:3.22 "$TEST_DATABASE_SCHEMA"

reset_schema
"$STACK" --env-file "$ENVIRONMENT_FILE" preflight
"$STACK" --env-file "$ENVIRONMENT_FILE" build
"$STACK" --env-file "$ENVIRONMENT_FILE" deploy

wait_for_http "http://127.0.0.1:$CONTROLLER_PORT/health/ready"
wait_for_http "http://127.0.0.1:$CONSOLE_PORT/console-health"
curl --fail --silent --show-error "http://127.0.0.1:$CONSOLE_PORT/health/ready" >/dev/null
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
second_network=$(create_network m52-after-backup 100.121.52.0/24)
second_id=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' <<<"$second_network")

cp "$BACKUP_DIRECTORY/m52-snapshot.dump" "$BACKUP_DIRECTORY/m52-tampered.dump"
sed 's/archive=m52-snapshot.dump/archive=m52-tampered.dump/' \
    "$BACKUP_DIRECTORY/m52-snapshot.manifest" >"$BACKUP_DIRECTORY/m52-tampered.manifest"
printf 'x' >>"$BACKUP_DIRECTORY/m52-tampered.dump"
chown 65532:65532 "$BACKUP_DIRECTORY/m52-tampered.dump" "$BACKUP_DIRECTORY/m52-tampered.manifest"
chmod 0600 "$BACKUP_DIRECTORY/m52-tampered.dump" "$BACKUP_DIRECTORY/m52-tampered.manifest"
if "$STACK" --env-file "$ENVIRONMENT_FILE" verify-backup m52-tampered >/dev/null 2>&1; then
    printf 'tampered backup was accepted\n' >&2
    exit 1
fi

"$STACK" --env-file "$ENVIRONMENT_FILE" restore m52-snapshot --confirm-schema "$TEST_DATABASE_SCHEMA"
networks_after_restore=$(list_networks)
python3 - "$first_id" "$second_id" "$networks_after_restore" <<'PY'
import json
import sys

networks = {item["id"] for item in json.loads(sys.argv[3])}
assert sys.argv[1] in networks
assert sys.argv[2] not in networks
PY

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
"$STACK" --env-file "$ENVIRONMENT_FILE" down
reset_schema

[[ -z $(docker ps -a --filter label=com.docker.compose.project=xs-nexus-dev --format '{{.ID}}') ]]
[[ $(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort) == "$DOCKER_NETWORKS_BEFORE" ]]
[[ $(docker network inspect 1panel-network --format '{{json .Containers}}') == "$NETWORK_MEMBERS_BEFORE" ]]
[[ $(ip -json route show default) == "$DEFAULT_ROUTES_BEFORE" ]]
[[ $(snapshot_firewall) == "$FIREWALL_BEFORE" ]]

printf 'Docker/1Panel deployment lifecycle test passed\n'
