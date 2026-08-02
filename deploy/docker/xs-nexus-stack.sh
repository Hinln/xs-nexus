#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"

fail() {
    printf 'xs-nexus-stack: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat >&2 <<'EOF'
usage: xs-nexus-stack.sh --env-file FILE COMMAND [ARGUMENTS]

commands:
  preflight
  build
  migrate
  deploy
  rollback
  backup NAME
  replicate-backup NAME
  fetch-backup NAME
  verify-backup NAME
  verify-backup-deep NAME --identity-file FILE
  restore NAME --confirm-schema SCHEMA --identity-file FILE
  prune-backups --confirm-before UTC_TIMESTAMP --identity-file FILE
  status
  down
EOF
    exit 2
}

[[ ${1:-} == --env-file && -n ${2:-} && -n ${3:-} ]] || usage
ENVIRONMENT_FILE=$2
COMMAND=$3
shift 3

[[ ! -L $ENVIRONMENT_FILE && -f $ENVIRONMENT_FILE ]] || fail 'environment file must be a regular non-symlink file'
ENVIRONMENT_FILE=$(realpath "$ENVIRONMENT_FILE")
environment_mode=$(stat -c '%a' "$ENVIRONMENT_FILE")
(( (8#$environment_mode & 022) == 0 )) || fail 'environment file must not be group or other writable'

COMPOSE=(docker compose --env-file "$ENVIRONMENT_FILE" -f "$COMPOSE_FILE")

file_value() {
    local file=$1 key=$2 count
    count=$(grep -Ec "^${key}=" "$file" || true)
    [[ $count -eq 1 ]] || fail "environment file must contain exactly one ${key} entry"
    sed -n "s/^${key}=//p" "$file"
}

environment_value() {
    file_value "$ENVIRONMENT_FILE" "$1"
}

validate_private_directory() {
    local path=$1 expected_owner=$2 mode owner
    [[ $path == /* && ! -L $path && -d $path ]] || fail "private directory is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    [[ $owner == "$expected_owner" ]] || fail "private directory owner is invalid: $path"
    (( (8#$mode & 077) == 0 )) || fail "private directory permissions are too broad: $path"
}

validate_private_file() {
    local path=$1 expected_owner=$2 mode owner size
    [[ $path == /* && ! -L $path && -f $path ]] || fail "secret file is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    size=$(stat -c '%s' "$path")
    [[ $owner == "$expected_owner" ]] || fail "secret file owner is invalid: $path"
    (( (8#$mode & 077) == 0 )) || fail "secret file permissions are too broad: $path"
    (( size > 0 && size <= 65536 )) || fail "secret file length is invalid: $path"
}

validate_state_directory() {
    local path=$1 mode
    [[ $path == /* && ! -L $path && -d $path ]] || fail "state directory is missing or invalid: $path"
    mode=$(stat -c '%a' "$path")
    (( (8#$mode & 077) == 0 )) || fail "state directory permissions are too broad: $path"
}

validate_port() {
    local value=$1 name=$2 numeric
    [[ $value =~ ^[1-9][0-9]{0,4}$ ]] || fail "$name must be a decimal port"
    numeric=$((10#$value))
    (( numeric <= 65535 )) || fail "$name is out of range"
}

validate_available_port() {
    local protocol=$1 port=$2 project=$3 service=$4 listeners own_binding
    if [[ $protocol == tcp ]]; then
        listeners=$(ss -H -ltn "sport = :$port" 2>/dev/null || true)
    else
        listeners=$(ss -H -lun "sport = :$port" 2>/dev/null || true)
    fi
    [[ -z $listeners ]] && return
    own_binding=$(docker ps \
        --filter "label=com.docker.compose.project=$project" \
        --filter "label=com.docker.compose.service=$service" \
        --format '{{.Ports}}' | grep -E ":${port}->[0-9]+/${protocol}" || true)
    [[ -n $own_binding ]] || fail "$protocol port is already in use: $port"
}

preflight() {
    local deployment project schema http_bind udp_bind controller_port console_port discovery_port relay_port
    local controller_secrets relay_secrets backup_directory replica_directory state_directory
    local local_retention replica_retention minimum_retained network_definition config_json marker target_id
    local -a required_variables=(
        XS_DEPLOYMENT
        XS_COMPOSE_PROJECT_NAME
        XS_RELEASE_REVISION
        XS_CONTROLLER_IMAGE
        XS_MIGRATION_IMAGE
        XS_RELAY_IMAGE
        XS_CONSOLE_IMAGE
        XS_DB_TOOLS_IMAGE
        XS_CONTROLLER_SECRETS_DIR
        XS_RELAY_SECRETS_DIR
        XS_BACKUP_DIR
        XS_BACKUP_REPLICA_DIR
        XS_BACKUP_LOCAL_RETENTION_DAYS
        XS_BACKUP_REPLICA_RETENTION_DAYS
        XS_BACKUP_MIN_RETAINED
        XS_STATE_DIR
        XS_DATABASE_SCHEMA
        XS_BIND_ADDRESS
        XS_UDP_BIND_ADDRESS
        XS_CONTROLLER_HTTP_PORT
        XS_CONSOLE_HTTP_PORT
        XS_DISCOVERY_UDP_PORT
        XS_RELAY_UDP_PORT
        XS_DISCOVERY_PUBLIC_ENDPOINT
        XS_RELAY_ID_BASE64
    )
    for variable in "${required_variables[@]}"; do
        [[ -n $(environment_value "$variable") ]] || fail "$variable must not be empty"
    done

    deployment=$(environment_value XS_DEPLOYMENT)
    project=$(environment_value XS_COMPOSE_PROJECT_NAME)
    schema=$(environment_value XS_DATABASE_SCHEMA)
    http_bind=$(environment_value XS_BIND_ADDRESS)
    udp_bind=$(environment_value XS_UDP_BIND_ADDRESS)
    controller_port=$(environment_value XS_CONTROLLER_HTTP_PORT)
    console_port=$(environment_value XS_CONSOLE_HTTP_PORT)
    discovery_port=$(environment_value XS_DISCOVERY_UDP_PORT)
    relay_port=$(environment_value XS_RELAY_UDP_PORT)
    [[ $deployment == dev || $deployment == rc ]] || fail 'XS_DEPLOYMENT must be dev or rc'
    [[ $project == "xs-nexus-$deployment" ]] || fail 'XS_COMPOSE_PROJECT_NAME must isolate the selected deployment'
    [[ $schema =~ ^[a-z_][a-z0-9_]{0,62}$ ]] || fail 'XS_DATABASE_SCHEMA is invalid'
    [[ $http_bind == 127.0.0.1 ]] || fail 'HTTP services must bind to 127.0.0.1 behind the TLS reverse proxy'
    [[ $udp_bind == 127.0.0.1 || $udp_bind == 0.0.0.0 ]] || fail 'UDP services must bind to 127.0.0.1 or 0.0.0.0'
    command -v ss >/dev/null 2>&1 || fail 'ss is required for host port validation'
    validate_port "$controller_port" XS_CONTROLLER_HTTP_PORT
    validate_port "$console_port" XS_CONSOLE_HTTP_PORT
    validate_port "$discovery_port" XS_DISCOVERY_UDP_PORT
    validate_port "$relay_port" XS_RELAY_UDP_PORT
    [[ $controller_port != "$console_port" ]] || fail 'Controller and Console HTTP ports must differ'
    [[ $discovery_port != "$relay_port" ]] || fail 'Discovery and Relay UDP ports must differ'
    validate_available_port tcp "$controller_port" "$project" controller
    validate_available_port tcp "$console_port" "$project" console
    validate_available_port udp "$discovery_port" "$project" controller
    validate_available_port udp "$relay_port" "$project" relay
    if [[ $deployment == dev ]]; then
        [[ $schema == *_dev ]] || fail 'development schema must end in _dev'
    else
        [[ $schema == *_rc ]] || fail 'RC schema must end in _rc'
        [[ -z $(git -C "$ROOT_DIR" status --porcelain) ]] || fail 'RC build requires a clean Git worktree'
        [[ $(environment_value XS_RELEASE_REVISION) == "$(git -C "$ROOT_DIR" rev-parse HEAD)" ]] || fail 'RC revision must equal the clean Git HEAD'
    fi

    controller_secrets=$(environment_value XS_CONTROLLER_SECRETS_DIR)
    relay_secrets=$(environment_value XS_RELAY_SECRETS_DIR)
    backup_directory=$(environment_value XS_BACKUP_DIR)
    replica_directory=$(environment_value XS_BACKUP_REPLICA_DIR)
    state_directory=$(environment_value XS_STATE_DIR)
    validate_private_directory "$controller_secrets" 65532
    validate_private_directory "$relay_secrets" 65532
    validate_private_directory "$backup_directory" 65532
    validate_private_directory "$replica_directory" 65532
    [[ $(stat -c '%d' "$backup_directory") != "$(stat -c '%d' "$replica_directory")" ]] || fail 'backup replica must be a distinct mounted filesystem'
    validate_state_directory "$state_directory"
    for file in database-url admin-api-token console-bootstrap-password credential-signing-key configuration-signing-key relay-catalog.json backup-recipient; do
        validate_private_file "$controller_secrets/$file" 65532
    done
    for file in controller-credential-public-key identity-key; do
        validate_private_file "$relay_secrets/$file" 65532
    done
    [[ $(<"$controller_secrets/backup-recipient") =~ ^age1[0-9a-z]{58}$ ]] || fail 'backup recipient is invalid'
    marker="$replica_directory/.xs-nexus-replica"
    validate_private_file "$marker" 65532
    [[ $(file_value "$marker" format) == xs-nexus-replica-v1 ]] || fail 'backup replica marker format is invalid'
    [[ $(file_value "$marker" deployment) == "$deployment" ]] || fail 'backup replica marker deployment is invalid'
    target_id=$(file_value "$marker" target_id)
    [[ $target_id =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$ ]] || fail 'backup replica target ID is invalid'
    local_retention=$(environment_value XS_BACKUP_LOCAL_RETENTION_DAYS)
    replica_retention=$(environment_value XS_BACKUP_REPLICA_RETENTION_DAYS)
    minimum_retained=$(environment_value XS_BACKUP_MIN_RETAINED)
    [[ $local_retention =~ ^[0-9]+$ && $replica_retention =~ ^[0-9]+$ && $minimum_retained =~ ^[0-9]+$ ]] || fail 'backup retention values are invalid'
    (( local_retention <= 3650 && replica_retention >= local_retention && replica_retention <= 3650 )) || fail 'backup retention values are out of range'
    (( minimum_retained >= 1 && minimum_retained <= 1000 )) || fail 'backup minimum retained count is out of range'
    if [[ $deployment == rc ]]; then
        (( local_retention >= 7 && replica_retention >= 30 && minimum_retained >= 3 )) || fail 'RC backup retention is below the required minimum'
    fi

    network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{.Driver}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
    [[ $network_definition == '1panel-network bridge 172.18.0.0/16 ' ]] || fail '1panel-network definition does not match the approved external network'

    config_json=$(mktemp)
    trap 'rm -f -- "$config_json"' RETURN
    "${COMPOSE[@]}" --profile '*' config --format json >"$config_json"
    python3 - "$config_json" <<'PY'
import json
import sys

configuration = json.load(open(sys.argv[1], encoding="utf-8"))
network = configuration.get("networks", {}).get("1panel-network", {})
assert network.get("external") is True
assert network.get("name") == "1panel-network"
services = configuration.get("services", {})
assert {"controller", "relay", "console", "migration", "db-tools", "network-probe"} <= set(services)
for name, service in services.items():
    user = str(service.get("user", ""))
    assert user and user.split(":", 1)[0] not in {"0", "root"}, name
    assert service.get("read_only") is True, name
    assert "ALL" in service.get("cap_drop", []), name
    assert "no-new-privileges:true" in service.get("security_opt", []), name
    assert "1panel-network" in service.get("networks", {}), name
    for port in service.get("ports", []):
        assert int(port.get("target", 0)) not in {3306, 5432, 6379}, name
assert not ({"postgres", "postgresql", "mysql", "redis"} & set(services))
PY
    trap - RETURN
    rm -f -- "$config_json"
}

verify_images() {
    local deployment revision image label
    deployment=$(environment_value XS_DEPLOYMENT)
    revision=$(environment_value XS_RELEASE_REVISION)
    for variable in XS_CONTROLLER_IMAGE XS_MIGRATION_IMAGE XS_RELAY_IMAGE XS_CONSOLE_IMAGE XS_DB_TOOLS_IMAGE; do
        image=$(environment_value "$variable")
        docker image inspect "$image" >/dev/null 2>&1 || fail "required image is unavailable: $image"
        if [[ $deployment == rc ]]; then
            label=$(docker image inspect "$image" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')
            [[ $label == "$revision" ]] || fail "RC image revision is invalid: $image"
        fi
    done
}

build_images() {
    preflight
    "${COMPOSE[@]}" --profile migration --profile ops build controller relay console db-tools
    verify_images
}

run_db_tools() {
    local -a options=()
    while [[ ${1:-} == -e || ${1:-} == -v ]]; do
        [[ -n ${2:-} ]] || fail 'missing db-tools option value'
        options+=("$1" "$2")
        shift 2
    done
    [[ $# -eq 1 ]] || fail 'invalid db-tools command'
    "${COMPOSE[@]}" --profile ops run --rm --no-deps "${options[@]}" db-tools "$1"
}

backup_schema_if_present() {
    local name=$1 status
    set +e
    run_db_tools schema-exists >/dev/null
    status=$?
    set -e
    case $status in
        0)
            run_db_tools -e "BACKUP_NAME=$name" backup
            ;;
        3)
            printf 'schema_backup=not_required\n'
            ;;
        *)
            fail 'unable to determine whether the database schema exists'
            ;;
    esac
}

migrate_database() {
    local migration_backup
    verify_images
    migration_backup="pre-migration-$(environment_value XS_DEPLOYMENT)-$(date -u +%Y%m%dT%H%M%S)-$$"
    backup_schema_if_present "$migration_backup"
    "${COMPOSE[@]}" --profile migration run --rm --no-deps migration
}

rollback_tag() {
    printf 'xs-nexus/%s-rollback-%s:previous' "$(environment_value XS_DEPLOYMENT)" "$1"
}

snapshot_running_images() {
    local state_directory rollback_file temporary service container image_id tag
    state_directory=$(environment_value XS_STATE_DIR)
    rollback_file="$state_directory/rollback-images.env"
    temporary=$(mktemp "$state_directory/.rollback-images.XXXXXX")
    for service in controller relay console; do
        container=$("${COMPOSE[@]}" ps -q "$service")
        if [[ -n $container ]]; then
            image_id=$(docker inspect "$container" --format '{{.Image}}')
            tag=$(rollback_tag "$service")
            docker image tag "$image_id" "$tag"
            printf '%s=%s\n' "${service^^}_IMAGE" "$tag" >>"$temporary"
        else
            printf '%s=none\n' "${service^^}_IMAGE" >>"$temporary"
        fi
    done
    chmod 0600 "$temporary"
    mv -- "$temporary" "$rollback_file"
}

rollback_images() {
    local state_directory rollback_file controller_image relay_image console_image
    state_directory=$(environment_value XS_STATE_DIR)
    rollback_file="$state_directory/rollback-images.env"
    [[ ! -L $rollback_file && -f $rollback_file ]] || return 1
    controller_image=$(file_value "$rollback_file" CONTROLLER_IMAGE)
    relay_image=$(file_value "$rollback_file" RELAY_IMAGE)
    console_image=$(file_value "$rollback_file" CONSOLE_IMAGE)
    [[ $controller_image != none && $relay_image != none && $console_image != none ]] || return 1
    env \
        XS_CONTROLLER_IMAGE="$controller_image" \
        XS_RELAY_IMAGE="$relay_image" \
        XS_CONSOLE_IMAGE="$console_image" \
        "${COMPOSE[@]}" up -d --no-build --pull never --remove-orphans --wait --wait-timeout 90 \
        controller relay console
}

write_active_record() {
    local state_directory record temporary service container image_id
    state_directory=$(environment_value XS_STATE_DIR)
    record="$state_directory/active-deployment.txt"
    temporary=$(mktemp "$state_directory/.active-deployment.XXXXXX")
    {
        printf 'deployment=%s\n' "$(environment_value XS_DEPLOYMENT)"
        printf 'revision=%s\n' "$(environment_value XS_RELEASE_REVISION)"
        printf 'activated_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        for service in controller relay console; do
            container=$("${COMPOSE[@]}" ps -q "$service")
            [[ -n $container ]] || fail "active service is missing: $service"
            image_id=$(docker inspect "$container" --format '{{.Image}}')
            printf '%s_image_id=%s\n' "$service" "$image_id"
        done
    } >"$temporary"
    chmod 0600 "$temporary"
    mv -- "$temporary" "$record"
}

deploy_stack() {
    preflight
    verify_images
    migrate_database
    snapshot_running_images
    if "${COMPOSE[@]}" up -d --no-build --pull never --remove-orphans --wait --wait-timeout 90 \
        controller relay console; then
        write_active_record
        return
    fi
    printf 'deployment activation failed; restoring previous images\n' >&2
    if rollback_images; then
        printf 'automatic_image_rollback=completed\n' >&2
    else
        "${COMPOSE[@]}" down --remove-orphans
        printf 'automatic_image_rollback=not_available\n' >&2
    fi
    return 1
}

restore_backup() {
    local name=${1:-} confirmation_flag=${2:-} confirmation=${3:-} identity_flag=${4:-} identity=${5:-}
    local schema container was_running=0 status
    [[ -n $name && $confirmation_flag == --confirm-schema && -n $confirmation && $identity_flag == --identity-file && -n $identity ]] || usage
    schema=$(environment_value XS_DATABASE_SCHEMA)
    [[ $confirmation == "$schema" ]] || fail 'restore confirmation does not match the configured schema'
    validate_private_file "$identity" 65532
    container=$("${COMPOSE[@]}" ps -q controller)
    if [[ -n $container ]]; then
        was_running=1
        "${COMPOSE[@]}" stop --timeout 20 controller
    fi
    set +e
    run_db_tools -e "BACKUP_NAME=$name" -e "CONFIRM_SCHEMA=$confirmation" \
        -e 'BACKUP_IDENTITY_FILE=/run/secrets/xs-backup/identity' \
        -v "$identity:/run/secrets/xs-backup/identity:ro" restore
    status=$?
    set -e
    if (( was_running == 1 )); then
        "${COMPOSE[@]}" up -d --no-deps --no-build --pull never --wait --wait-timeout 90 controller
    fi
    (( status == 0 )) || fail 'database restore failed'
}

verify_backup_deep() {
    local name=${1:-} identity_flag=${2:-} identity=${3:-}
    [[ -n $name && $identity_flag == --identity-file && -n $identity ]] || usage
    validate_private_file "$identity" 65532
    run_db_tools -e "BACKUP_NAME=$name" \
        -e 'BACKUP_IDENTITY_FILE=/run/secrets/xs-backup/identity' \
        -v "$identity:/run/secrets/xs-backup/identity:ro" verify-deep
}

prune_backups() {
    local confirmation_flag=${1:-} confirmation=${2:-} identity_flag=${3:-} identity=${4:-}
    [[ $confirmation_flag == --confirm-before && -n $confirmation && $identity_flag == --identity-file && -n $identity ]] || usage
    validate_private_file "$identity" 65532
    run_db_tools -e "CONFIRM_BEFORE=$confirmation" \
        -e 'BACKUP_IDENTITY_FILE=/run/secrets/xs-backup/identity' \
        -v "$identity:/run/secrets/xs-backup/identity:ro" prune
}

case $COMMAND in
    preflight)
        [[ $# -eq 0 ]] || usage
        preflight
        ;;
    build)
        [[ $# -eq 0 ]] || usage
        build_images
        ;;
    migrate)
        [[ $# -eq 0 ]] || usage
        preflight
        migrate_database
        ;;
    deploy)
        [[ $# -eq 0 ]] || usage
        deploy_stack
        ;;
    rollback)
        [[ $# -eq 0 ]] || usage
        preflight
        rollback_images || fail 'no complete previous image set is available'
        write_active_record
        ;;
    backup)
        [[ $# -eq 1 ]] || usage
        preflight
        verify_images
        run_db_tools -e "BACKUP_NAME=$1" backup
        ;;
    replicate-backup)
        [[ $# -eq 1 ]] || usage
        preflight
        verify_images
        run_db_tools -e "BACKUP_NAME=$1" replicate
        ;;
    fetch-backup)
        [[ $# -eq 1 ]] || usage
        preflight
        verify_images
        run_db_tools -e "BACKUP_NAME=$1" fetch
        ;;
    verify-backup)
        [[ $# -eq 1 ]] || usage
        preflight
        verify_images
        run_db_tools -e "BACKUP_NAME=$1" verify
        ;;
    verify-backup-deep)
        preflight
        verify_images
        verify_backup_deep "$@"
        ;;
    restore)
        preflight
        verify_images
        restore_backup "$@"
        ;;
    prune-backups)
        preflight
        verify_images
        prune_backups "$@"
        ;;
    status)
        [[ $# -eq 0 ]] || usage
        preflight
        "${COMPOSE[@]}" ps
        ;;
    down)
        [[ $# -eq 0 ]] || usage
        preflight
        "${COMPOSE[@]}" down --remove-orphans
        ;;
    *)
        usage
        ;;
esac
