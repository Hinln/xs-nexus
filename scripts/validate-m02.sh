#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
QA_DIR="$ROOT_DIR/artifacts/qa/m0.2-$STAMP"
LOG_FILE="$QA_DIR/validate-m02.log"
SUMMARY_FILE="$QA_DIR/summary.txt"

mkdir -p "$QA_DIR"

finish() {
    local status=$?
    printf 'validation_status=%s\n' "$status" | tee "$SUMMARY_FILE"
    printf 'evidence=%s\n' "$QA_DIR" | tee -a "$SUMMARY_FILE"
}
trap finish EXIT

cd "$ROOT_DIR"
exec > >(tee "$LOG_FILE") 2>&1

printf 'M0.2 validation started at %s\n' "$(date -u --iso-8601=seconds)"
env GIT_PAGER=cat git diff --check
make fmt-check
make lint
make build
make test
make test-network
make security-check
npm audit --audit-level=high
docker compose -f deploy/docker/compose.yaml config
network_definition=$(docker network inspect 1panel-network --format '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}')
printf '%s\n' "$network_definition" | grep -Fx '1panel-network 172.18.0.0/16 '
printf 'M0.2 validation passed at %s\n' "$(date -u --iso-8601=seconds)"
