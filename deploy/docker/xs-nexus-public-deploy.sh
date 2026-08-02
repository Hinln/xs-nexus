#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
STACK="$ROOT_DIR/deploy/docker/xs-nexus-stack.sh"
EDGE="$ROOT_DIR/deploy/docker/xs-nexus-edge.sh"

usage() {
    printf 'usage: xs-nexus-public-deploy.sh --env-file FILE build|preflight|deploy|status|down\n' >&2
    exit 2
}

[[ ${1:-} == --env-file && -n ${2:-} && -n ${3:-} ]] || usage
ENVIRONMENT_FILE=$2
COMMAND=$3
shift 3
[[ $# -eq 0 ]] || usage

case $COMMAND in
    build)
        "$STACK" --env-file "$ENVIRONMENT_FILE" build
        "$EDGE" --env-file "$ENVIRONMENT_FILE" pull
        "$EDGE" --env-file "$ENVIRONMENT_FILE" preflight
        ;;
    preflight)
        "$STACK" --env-file "$ENVIRONMENT_FILE" preflight
        "$EDGE" --env-file "$ENVIRONMENT_FILE" preflight
        ;;
    deploy)
        "$STACK" --env-file "$ENVIRONMENT_FILE" deploy
        "$EDGE" --env-file "$ENVIRONMENT_FILE" deploy
        ;;
    status)
        "$STACK" --env-file "$ENVIRONMENT_FILE" status
        "$EDGE" --env-file "$ENVIRONMENT_FILE" status
        ;;
    down)
        "$EDGE" --env-file "$ENVIRONMENT_FILE" down
        "$STACK" --env-file "$ENVIRONMENT_FILE" down
        ;;
    *)
        usage
        ;;
esac
