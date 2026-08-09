#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly ROOT_DIR
readonly EVIDENCE_INPUT="${1:?usage: test-console-real-e2e.sh EVIDENCE_DIR}"
readonly DATABASE_URL_VALUE="${XS_TEST_DATABASE_URL:?XS_TEST_DATABASE_URL is required}"
readonly TEST_DATABASE_SCHEMA=xs_nexus_console_e2e_test
readonly CONTROLLER_URL=http://127.0.0.1:8080
readonly CONSOLE_USERNAME=admin

mkdir -p "$EVIDENCE_INPUT"
EVIDENCE_DIR=$(cd "$EVIDENCE_INPUT" && pwd)
readonly EVIDENCE_DIR
TEMPORARY=$(mktemp -d)
readonly TEMPORARY
readonly CONTROLLER_LOG="$EVIDENCE_DIR/controller.log"
controller_pid=''
test_status=1

cleanup() {
    local exit_status=$?
    trap - EXIT
    if [[ -n $controller_pid ]] && kill -0 "$controller_pid" 2>/dev/null; then
        kill "$controller_pid" 2>/dev/null || true
        wait "$controller_pid" 2>/dev/null || true
    fi
    if [[ -x "$ROOT_DIR/target/debug/examples/reset_test_schema" ]]; then
        DATABASE_URL="$DATABASE_URL_VALUE" DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
            "$ROOT_DIR/target/debug/examples/reset_test_schema" >/dev/null 2>&1 || true
    fi
    if (( test_status == 0 )); then
        printf 'status=PASS\n' >"$EVIDENCE_DIR/status.txt"
    else
        printf 'status=FAIL\n' >"$EVIDENCE_DIR/status.txt"
    fi
    rm -rf "$TEMPORARY"
    exit "$exit_status"
}
trap cleanup EXIT

umask 077
openssl rand -hex 32 >"$TEMPORARY/admin-api-token"
openssl rand -base64 24 | tr -d '\n' >"$TEMPORARY/console-password"
head -c 32 /dev/urandom >"$TEMPORARY/credential-signing-key"
head -c 32 /dev/urandom >"$TEMPORARY/configuration-signing-key"
printf '%s' "$DATABASE_URL_VALUE" >"$TEMPORARY/database-url"

cd "$ROOT_DIR"
cargo build -p xs-controller --bin xs-controller --example reset_test_schema
DATABASE_URL="$DATABASE_URL_VALUE" DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
    "$ROOT_DIR/target/debug/examples/reset_test_schema"
database_expected_role=$(python3 -c 'import sys; from urllib.parse import urlsplit; print(urlsplit(sys.stdin.read()).username or "")' <<<"$DATABASE_URL_VALUE")
[[ $database_expected_role =~ ^[a-z_][a-z0-9_]{0,62}$ ]]
DATABASE_URL_FILE="$TEMPORARY/database-url" DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
    "$ROOT_DIR/target/debug/xs-controller" migrate

CONTROLLER_LISTEN=127.0.0.1:8080 \
DATABASE_URL_FILE="$TEMPORARY/database-url" \
DATABASE_SCHEMA="$TEST_DATABASE_SCHEMA" \
DATABASE_EXPECTED_ROLE="$database_expected_role" \
ADMIN_API_TOKEN_FILE="$TEMPORARY/admin-api-token" \
CONSOLE_BOOTSTRAP_USERNAME="$CONSOLE_USERNAME" \
CONSOLE_BOOTSTRAP_PASSWORD_FILE="$TEMPORARY/console-password" \
CONSOLE_COOKIE_SECURE=false \
CREDENTIAL_SIGNING_KEY_PATH="$TEMPORARY/credential-signing-key" \
CONFIG_SIGNING_KEY_PATH="$TEMPORARY/configuration-signing-key" \
RUST_LOG=warn \
    "$ROOT_DIR/target/debug/xs-controller" >"$CONTROLLER_LOG" 2>&1 &
controller_pid=$!

for _ in {1..100}; do
    if curl --fail --silent "$CONTROLLER_URL/health/ready" >/dev/null; then
        break
    fi
    if ! kill -0 "$controller_pid" 2>/dev/null; then
        cat "$CONTROLLER_LOG" >&2
        exit 1
    fi
    sleep 0.1
done
curl --fail --silent "$CONTROLLER_URL/health/ready" >/dev/null

{
    printf 'commit=%s\n' "$(git rev-parse HEAD)"
    printf 'tested_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'controller_sha256=%s\n' "$(sha256sum target/debug/xs-controller | awk '{print $1}')"
    printf 'schema=%s\n' "$TEST_DATABASE_SCHEMA"
    rustc --version
    node --version
    npm --version
    npx --no-install playwright --version
} >"$EVIDENCE_DIR/environment.txt"

XS_CONSOLE_E2E_USERNAME="$CONSOLE_USERNAME" \
XS_CONSOLE_E2E_PASSWORD_FILE="$TEMPORARY/console-password" \
XS_CONSOLE_E2E_EVIDENCE_DIR="$EVIDENCE_DIR" \
    npm run test:e2e:real

test_status=0
printf 'real Console E2E passed\n'
