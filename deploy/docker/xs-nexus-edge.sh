#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
BASE_COMPOSE="$ROOT_DIR/deploy/docker/compose.yaml"
EDGE_COMPOSE="$ROOT_DIR/deploy/docker/edge.compose.yaml"

fail() {
    printf 'xs-nexus-edge: %s\n' "$1" >&2
    exit 1
}

usage() {
    printf 'usage: xs-nexus-edge.sh --env-file FILE build|preflight|deploy|reload|status|down\n' >&2
    exit 2
}

[[ ${1:-} == --env-file && -n ${2:-} && -n ${3:-} ]] || usage
ENVIRONMENT_FILE=$2
COMMAND=$3
shift 3
[[ $# -eq 0 ]] || usage

[[ ! -L $ENVIRONMENT_FILE && -f $ENVIRONMENT_FILE ]] || fail 'environment file must be a regular non-symlink file'
ENVIRONMENT_FILE=$(realpath "$ENVIRONMENT_FILE")
environment_mode=$(stat -c '%a' "$ENVIRONMENT_FILE")
(( (8#$environment_mode & 022) == 0 )) || fail 'environment file must not be group or other writable'

COMPOSE=(docker compose --env-file "$ENVIRONMENT_FILE" -f "$BASE_COMPOSE" -f "$EDGE_COMPOSE")

environment_value() {
    local key=$1 count
    count=$(grep -Ec "^${key}=" "$ENVIRONMENT_FILE" || true)
    [[ $count -eq 1 ]] || fail "environment file must contain exactly one ${key} entry"
    sed -n "s/^${key}=//p" "$ENVIRONMENT_FILE"
}

validate_private_path() {
    local path=$1 expected_type=$2 expected_owner=$3 mode owner
    [[ $path == /* && ! -L $path ]] || fail "edge path is invalid: $path"
    if [[ $expected_type == file ]]; then
        [[ -f $path ]] || fail "edge file is missing: $path"
    else
        [[ -d $path ]] || fail "edge directory is missing: $path"
    fi
    mode=$(stat -c '%a' "$path")
    owner=$(stat -c '%u' "$path")
    [[ $owner == "$expected_owner" ]] || fail "edge path owner is invalid: $path"
    (( (8#$mode & 022) == 0 )) || fail "edge path is group or other writable: $path"
}

validate_available_port() {
    local port=$1 project=$2 listeners own_binding
    listeners=$(ss -H -ltn "sport = :$port" 2>/dev/null || true)
    [[ -z $listeners ]] && return
    own_binding=$(docker ps \
        --filter "label=com.docker.compose.project=$project" \
        --filter "label=com.docker.compose.service=edge" \
        --format '{{.Ports}}' | grep -E ":${port}->[0-9]+/tcp" || true)
    [[ -n $own_binding ]] || fail "TCP port is already in use: $port"
}

preflight() {
    local project deployment image revision label config data bind_address http_port https_port http_port_number https_port_number
    local network_definition config_json
    project=$(environment_value XS_COMPOSE_PROJECT_NAME)
    deployment=$(environment_value XS_DEPLOYMENT)
    image=$(environment_value XS_EDGE_IMAGE)
    revision=$(environment_value XS_RELEASE_REVISION)
    config=$(environment_value XS_EDGE_CONFIG_FILE)
    data=$(environment_value XS_EDGE_DATA_DIR)
    bind_address=$(environment_value XS_EDGE_BIND_ADDRESS)
    http_port=$(environment_value XS_EDGE_HTTP_PORT)
    https_port=$(environment_value XS_EDGE_HTTPS_PORT)

    [[ $deployment == dev || $deployment == rc ]] || fail 'XS_DEPLOYMENT must be dev or rc'
    [[ $project == "xs-nexus-$deployment" ]] || fail 'edge project must match the application deployment'
    [[ $image =~ ^xs-nexus/edge:[A-Za-z0-9._-]+$ ]] || fail 'edge image must use an isolated XS Nexus tag'
    docker image inspect "$image" >/dev/null 2>&1 || fail 'pinned edge image is unavailable'
    label=$(docker image inspect "$image" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')
    [[ -n $revision && $label == "$revision" ]] || fail 'edge image revision does not match the deployment revision'
    [[ $bind_address == 0.0.0.0 || $bind_address == 127.0.0.1 ]] || fail 'edge bind address is invalid'
    [[ $http_port =~ ^[1-9][0-9]{0,4}$ && $https_port =~ ^[1-9][0-9]{0,4}$ ]] || fail 'edge ports must be decimal integers'
    http_port_number=$((10#$http_port))
    https_port_number=$((10#$https_port))
    (( http_port_number <= 65535 && https_port_number <= 65535 && http_port_number != https_port_number )) || fail 'edge ports are invalid'
    command -v ss >/dev/null 2>&1 || fail 'ss is required for host port validation'
    validate_available_port "$http_port" "$project"
    validate_available_port "$https_port" "$project"
    validate_private_path "$config" file 0
    validate_private_path "$data" directory 65532

    network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{.Driver}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
    [[ $network_definition == '1panel-network bridge 172.18.0.0/16 ' ]] || fail '1panel-network definition does not match the approved external network'

    config_json=$(mktemp)
    trap 'rm -f -- "$config_json"' RETURN
    "${COMPOSE[@]}" config --format json >"$config_json"
    python3 - "$config_json" <<'PY'
import json
import sys

configuration = json.load(open(sys.argv[1], encoding="utf-8"))
edge = configuration["services"]["edge"]
assert edge["user"].split(":", 1)[0] not in {"0", "root"}
assert edge["read_only"] is True
assert "ALL" in edge["cap_drop"]
assert "no-new-privileges:true" in edge["security_opt"]
assert "1panel-network" in edge["networks"]
ports = {(int(item["target"]), int(item["published"])) for item in edge["ports"]}
assert len(ports) == 2
assert {target for target, _published in ports} == {8080, 8443}
PY
    trap - RETURN
    rm -f -- "$config_json"

    "${COMPOSE[@]}" run --rm --no-deps --entrypoint caddy edge \
        validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null
}

require_application() {
    local service container health
    for service in controller console; do
        container=$("${COMPOSE[@]}" ps -q "$service")
        [[ -n $container ]] || fail "application service is missing: $service"
        health=$(docker inspect "$container" --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}')
        [[ $health == healthy ]] || fail "application service is not healthy: $service"
    done
}

case $COMMAND in
    build)
        SOURCE_DATE_EPOCH=$(git -C "$ROOT_DIR" show -s --format=%ct HEAD)
        export SOURCE_DATE_EPOCH
        "${COMPOSE[@]}" build edge
        preflight
        ;;
    preflight)
        preflight
        ;;
    deploy)
        preflight
        require_application
        "${COMPOSE[@]}" up -d --no-deps --no-build --pull never --wait --wait-timeout 90 edge
        ;;
    reload)
        preflight
        require_application
        "${COMPOSE[@]}" exec -T edge caddy reload --config /etc/caddy/Caddyfile --adapter caddyfile
        ;;
    status)
        preflight
        "${COMPOSE[@]}" ps edge
        ;;
    down)
        preflight
        "${COMPOSE[@]}" stop --timeout 15 edge
        "${COMPOSE[@]}" rm -f edge
        ;;
    *)
        usage
        ;;
esac
