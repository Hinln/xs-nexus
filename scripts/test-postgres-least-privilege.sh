#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
POSTGRES_IMAGE=${XS_TEST_POSTGRES_IMAGE:-postgres@sha256:774521500f4c22761b25a6bdb772a0a3c2e8dd32468210bdad9231c5752ea398}
RUST_IMAGE=${XS_TEST_RUST_IMAGE:-xs-nexus/qa-rust:1.94.0}
SUFFIX=$(openssl rand -hex 4)
POSTGRES_CONTAINER="xs-gate16-postgres-$SUFFIX"
CONTROLLER_CONTAINER="xs-gate16-controller-$SUFFIX"
TEMPORARY=$(mktemp -d /tmp/xs-gate16-sql.XXXXXX)
NETWORK_ID_BEFORE=$(docker network inspect 1panel-network --format '{{.Id}}')

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    docker rm -f "$CONTROLLER_CONTAINER" "$POSTGRES_CONTAINER" >/dev/null 2>&1 || true
    rm -rf -- "$TEMPORARY"
    if [[ $(docker network inspect 1panel-network --format '{{.Id}}' 2>/dev/null || true) != "$NETWORK_ID_BEFORE" ]]; then
        printf '1panel-network identity changed during PostgreSQL least-privilege test\n' >&2
        status=1
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

docker image inspect "$POSTGRES_IMAGE" >/dev/null
docker image inspect "$RUST_IMAGE" >/dev/null
[[ $(docker network inspect 1panel-network --format '{{.Name}} {{.Driver}} {{range .IPAM.Config}}{{.Subnet}} {{end}}') == '1panel-network bridge 172.18.0.0/16 ' ]]

umask 077
openssl rand -hex 32 >"$TEMPORARY/bootstrap-password"
openssl rand -hex 32 >"$TEMPORARY/app-password"
openssl rand -hex 32 >"$TEMPORARY/migrator-password"
openssl rand -hex 32 >"$TEMPORARY/admin-token"
head -c 32 /dev/urandom >"$TEMPORARY/credential-key"
head -c 32 /dev/urandom >"$TEMPORARY/config-key"
python3 - "$TEMPORARY" "$POSTGRES_CONTAINER" <<'PY'
import sys
from pathlib import Path
from urllib.parse import quote

root = Path(sys.argv[1])
host = sys.argv[2]


def secret(name):
    return (root / name).read_text(encoding="utf-8").strip()


(root / "bootstrap-url").write_text(
    f"postgresql://gate16_bootstrap:{quote(secret('bootstrap-password'), safe='')}@{host}:5432/gate16",
    encoding="utf-8",
)
(root / "app-url").write_text(
    f"postgresql://gate16_app:{quote(secret('app-password'), safe='')}@{host}:5432/gate16",
    encoding="utf-8",
)
(root / "migrator-url").write_text(
    f"postgresql://gate16_migrator:{quote(secret('migrator-password'), safe='')}@{host}:5432/gate16",
    encoding="utf-8",
)
PY
chmod 0400 "$TEMPORARY"/*
POSTGRES_UID=$(docker run --rm --entrypoint id "$POSTGRES_IMAGE" -u postgres)
chown -R "$POSTGRES_UID:$POSTGRES_UID" "$TEMPORARY"

docker run -d --rm --name "$POSTGRES_CONTAINER" --network 1panel-network \
    --mount "type=bind,src=$TEMPORARY,dst=/run/test-secrets,readonly" \
    --mount "type=bind,src=$ROOT_DIR,dst=/workspace,readonly" \
    -e POSTGRES_USER=gate16_bootstrap \
    -e POSTGRES_DB=gate16 \
    -e POSTGRES_PASSWORD_FILE=/run/test-secrets/bootstrap-password \
    "$POSTGRES_IMAGE" >/dev/null
for _ in $(seq 1 60); do
    if docker exec "$POSTGRES_CONTAINER" pg_isready -q -U gate16_bootstrap -d gate16; then
        break
    fi
    sleep 1
done
docker exec "$POSTGRES_CONTAINER" pg_isready -q -U gate16_bootstrap -d gate16

cargo_mounts=(
    --mount "type=bind,src=$ROOT_DIR,dst=$ROOT_DIR"
    --mount "type=bind,src=$TEMPORARY,dst=/run/test-secrets,readonly"
)
if [[ -d /srv/xs-nexus-qa/cache/cargo-registry ]]; then
    cargo_mounts+=(--volume /srv/xs-nexus-qa/cache/cargo-registry:/usr/local/cargo/registry)
fi
if [[ -d /srv/xs-nexus-qa/cache/cargo-git ]]; then
    cargo_mounts+=(--volume /srv/xs-nexus-qa/cache/cargo-git:/usr/local/cargo/git)
fi

run_migration() {
    local database_file=$1 owner_role=${2:-}
    local -a environment=(
        -e "DATABASE_URL_FILE=/run/test-secrets/$database_file"
        -e DATABASE_SCHEMA=gate16_schema
    )
    if [[ -n $owner_role ]]; then
        environment+=(
            -e "DATABASE_OWNER_ROLE=$owner_role"
            -e DATABASE_APP_ROLE=gate16_app
        )
    fi
    docker run --rm --network 1panel-network \
        "${cargo_mounts[@]}" \
        --workdir "$ROOT_DIR" \
        "${environment[@]}" \
        "$RUST_IMAGE" cargo run --quiet -p xs-controller --bin xs-controller -- migrate
}

run_role_hardening() {
    docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
        export PGPASSWORD=$(cat /run/test-secrets/bootstrap-password)
        export XS_DATABASE_APP_PASSWORD=$(cat /run/test-secrets/app-password)
        export XS_DATABASE_MIGRATOR_PASSWORD=$(cat /run/test-secrets/migrator-password)
        psql -q -h 127.0.0.1 -U gate16_bootstrap -d gate16 \
            -v schema=gate16_schema -v owner_role=gate16_owner \
            -v app_role=gate16_app -v migrator_role=gate16_migrator \
            -f /workspace/deploy/docker/postgres-role-hardening.sql
    '
}

run_migration bootstrap-url
run_role_hardening
docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
    export PGPASSWORD=$(cat /run/test-secrets/bootstrap-password)
    psql -q -h 127.0.0.1 -U gate16_bootstrap -d gate16 \
        -c "DROP SCHEMA gate16_schema CASCADE"
'
run_role_hardening
run_migration migrator-url gate16_owner
docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
    export PGPASSWORD=$(cat /run/test-secrets/bootstrap-password)
    psql -q -h 127.0.0.1 -U gate16_bootstrap -d gate16 \
        -c "DROP SCHEMA gate16_schema CASCADE"
'
run_migration migrator-url gate16_owner

docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
    export PGPASSWORD=$(cat /run/test-secrets/app-password)
    psql -q -h 127.0.0.1 -U gate16_app -d gate16 \
        -v schema=gate16_schema -v owner_role=gate16_owner -v app_role=gate16_app \
        -f /workspace/scripts/verify-postgres-least-privilege.sql
'

docker run -d --rm --name "$CONTROLLER_CONTAINER" --network 1panel-network \
    --entrypoint "$ROOT_DIR/target/debug/xs-controller" \
    --mount "type=bind,src=$ROOT_DIR,dst=$ROOT_DIR,readonly" \
    --mount "type=bind,src=$TEMPORARY,dst=/run/test-secrets,readonly" \
    -e CONTROLLER_LISTEN=0.0.0.0:8080 \
    -e DATABASE_URL_FILE=/run/test-secrets/app-url \
    -e DATABASE_SCHEMA=gate16_schema \
    -e DATABASE_EXPECTED_ROLE=gate16_app \
    -e ADMIN_API_TOKEN_FILE=/run/test-secrets/admin-token \
    -e CONSOLE_COOKIE_SECURE=false \
    -e CREDENTIAL_SIGNING_KEY_PATH=/run/test-secrets/credential-key \
    -e CONFIG_SIGNING_KEY_PATH=/run/test-secrets/config-key \
    "$RUST_IMAGE" serve >/dev/null
for _ in $(seq 1 60); do
    if docker exec "$CONTROLLER_CONTAINER" "$ROOT_DIR/target/debug/xs-controller" healthcheck >/dev/null 2>&1; then
        break
    fi
    docker inspect "$CONTROLLER_CONTAINER" >/dev/null
    sleep 1
done
docker exec "$CONTROLLER_CONTAINER" "$ROOT_DIR/target/debug/xs-controller" healthcheck >/dev/null

negative_sql() {
    local sql=$1
    if docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
        export PGPASSWORD=$(cat /run/test-secrets/app-password)
        exec psql -q -h 127.0.0.1 -U gate16_app -d gate16 -v ON_ERROR_STOP=1 -c "$1"
    ' sh "$sql" >/dev/null 2>&1; then
        printf 'runtime role unexpectedly executed forbidden SQL\n' >&2
        exit 1
    fi
}

negative_sql 'CREATE DATABASE gate16_forbidden'
negative_sql 'CREATE ROLE gate16_forbidden'
negative_sql 'CREATE SCHEMA gate16_forbidden'
negative_sql 'CREATE TABLE public.gate16_forbidden(id integer)'
negative_sql 'SELECT rolpassword FROM pg_catalog.pg_authid'
negative_sql 'BEGIN; ALTER TABLE gate16_schema.networks ADD COLUMN gate16_forbidden integer; ROLLBACK'
negative_sql "INSERT INTO gate16_schema._sqlx_migrations (version, description, installed_on, success, checksum, execution_time) VALUES (999999, 'forbidden', now(), true, decode('00', 'hex'), 0)"
negative_sql 'UPDATE gate16_schema._sqlx_migrations SET success = false'
negative_sql 'DELETE FROM gate16_schema._sqlx_migrations'

printf '%s\n' \
    'migration_as_owner=pass' \
    'runtime_as_app=pass' \
    'negative_permissions=pass' \
    'ephemeral_database_isolated=pass'
