#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE="$ROOT_DIR/deploy/docker/compose.yaml"
NETWORK_NAME=1panel-network
SENTINEL_NAME=xs-onepanel-ci-sentinel
RUN_ID=${GITHUB_RUN_ID:-}
TEMPORARY=$(mktemp -d)
ENVIRONMENT_FILE="$TEMPORARY/coexistence.env"
NETWORK_ID=''
SENTINEL_ID=''
NETWORKS_BEFORE=''
DEFAULT_ROUTE_BEFORE=''

cleanup() {
    local status=$? label
    set +e
    if [[ -n $SENTINEL_ID ]]; then
        label=$(docker inspect "$SENTINEL_ID" --format '{{index .Config.Labels "io.xs-nexus.onepanel-fixture"}}' 2>/dev/null || true)
        if [[ $label == "$RUN_ID" ]]; then
            docker rm -f "$SENTINEL_ID" >/dev/null 2>&1 || true
        fi
    fi
    if [[ -n $NETWORK_ID ]]; then
        label=$(docker network inspect "$NETWORK_ID" --format '{{index .Labels "io.xs-nexus.onepanel-fixture"}}' 2>/dev/null || true)
        if [[ $label == "$RUN_ID" ]]; then
            docker network rm "$NETWORK_ID" >/dev/null 2>&1 || true
        fi
    fi
    rm -rf -- "$TEMPORARY"
    if [[ -n $NETWORKS_BEFORE ]]; then
        [[ $(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort) == "$NETWORKS_BEFORE" ]] || status=1
    fi
    if [[ -n $DEFAULT_ROUTE_BEFORE ]]; then
        [[ $(ip -json route show default) == "$DEFAULT_ROUTE_BEFORE" ]] || status=1
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

[[ ${GITHUB_ACTIONS:-} == true ]]
[[ ${XS_ONEPANEL_CI_FIXTURE:-} == 1 ]]
[[ $RUN_ID =~ ^[0-9]+$ ]]
if docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then
    printf 'refusing to run when 1panel-network already exists\n' >&2
    exit 1
fi

NETWORKS_BEFORE=$(docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort)
DEFAULT_ROUTE_BEFORE=$(ip -json route show default)
NETWORK_ID=$(docker network create \
    --driver bridge \
    --subnet 172.18.0.0/16 \
    --label "io.xs-nexus.onepanel-fixture=$RUN_ID" \
    "$NETWORK_NAME")
[[ $(docker network inspect "$NETWORK_ID" --format '{{.Name}} {{.Driver}} {{range .IPAM.Config}}{{.Subnet}} {{end}}') == '1panel-network bridge 172.18.0.0/16 ' ]]

SENTINEL_ID=$(docker run -d \
    --name "$SENTINEL_NAME" \
    --network "$NETWORK_NAME" \
    --restart unless-stopped \
    --label "io.xs-nexus.onepanel-fixture=$RUN_ID" \
    alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce \
    sleep 600)

cat >"$ENVIRONMENT_FILE" <<EOF
XS_DEPLOYMENT=dev
XS_COMPOSE_PROJECT_NAME=xs-nexus-coexistence-ci
XS_RELEASE_REVISION=${GITHUB_SHA:-0000000000000000000000000000000000000000}
XS_CONTROLLER_IMAGE=alpine:3.22
XS_MIGRATION_IMAGE=alpine:3.22
XS_RELAY_IMAGE=alpine:3.22
XS_CONSOLE_IMAGE=alpine:3.22
XS_DB_TOOLS_IMAGE=alpine:3.22
XS_CONTROLLER_SECRETS_DIR=$TEMPORARY/controller
XS_DATABASE_SECRETS_DIR=$TEMPORARY/database
XS_RELAY_SECRETS_DIR=$TEMPORARY/relay
XS_BACKUP_DIR=$TEMPORARY/backups
XS_BACKUP_REPLICA_DIR=$TEMPORARY/replica
XS_DATABASE_SCHEMA=xs_nexus_coexistence_ci
XS_DATABASE_APP_ROLE=xs_app
XS_DATABASE_OWNER_ROLE=xs_owner
XS_DISCOVERY_PUBLIC_ENDPOINT=127.0.0.1:42000
XS_RELAY_ID_BASE64=AAAAAAAAAAAAAAAAAAAAAA==
EOF
chmod 0600 "$ENVIRONMENT_FILE"

docker compose --env-file "$ENVIRONMENT_FILE" -f "$COMPOSE_FILE" down --remove-orphans
[[ $(docker network inspect "$NETWORK_NAME" --format '{{.Id}}') == "$NETWORK_ID" ]]
[[ $(docker inspect "$SENTINEL_NAME" --format '{{.Id}}') == "$SENTINEL_ID" ]]
[[ $(docker inspect "$SENTINEL_NAME" --format '{{.State.Running}}') == true ]]

sudo systemctl restart docker
for _attempt in $(seq 1 60); do
    if docker info >/dev/null 2>&1 \
        && [[ $(docker inspect "$SENTINEL_NAME" --format '{{.State.Running}}' 2>/dev/null || true) == true ]]; then
        break
    fi
    sleep 1
done

[[ $(docker network inspect "$NETWORK_NAME" --format '{{.Id}}') == "$NETWORK_ID" ]]
[[ $(docker network inspect "$NETWORK_NAME" --format '{{range $id, $_ := .Containers}}{{$id}} {{end}}') == *"$SENTINEL_ID"* ]]
[[ $(docker inspect "$SENTINEL_NAME" --format '{{.Id}}') == "$SENTINEL_ID" ]]
[[ $(docker inspect "$SENTINEL_NAME" --format '{{.State.Running}}') == true ]]
[[ $(ip -json route show default) == "$DEFAULT_ROUTE_BEFORE" ]]

printf 'compose_external_network_preserved=true\n'
printf 'docker_daemon_restart_preserved_network=true\n'
printf 'docker_daemon_restart_preserved_sentinel=true\n'
printf 'host_default_route_preserved=true\n'
printf '1Panel external-network runtime fixture passed\n'
