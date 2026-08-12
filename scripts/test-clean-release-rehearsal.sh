#!/usr/bin/env bash
set -Eeuo pipefail
export LC_ALL=C

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
SOURCE_REPOSITORY=${SOURCE_REPOSITORY:-$ROOT_DIR}
EXPECTED_REVISION=${EXPECTED_REVISION:-}
EVIDENCE_DIR=${EVIDENCE_DIR:-$ROOT_DIR/artifacts/clean-release-rehearsal}
POSTGRES_IMAGE=${XS_GATE25_POSTGRES_IMAGE:-postgres:18-alpine3.22@sha256:774521500f4c22761b25a6bdb772a0a3c2e8dd32468210bdad9231c5752ea398}
ALPINE_IMAGE=${XS_GATE25_ALPINE_IMAGE:-alpine:3.22@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce}
RUN_TOKEN=$(openssl rand -hex 4)
TEMPORARY=$(mktemp -d /tmp/xs-gate25.XXXXXXXX)
CHECKOUT=$TEMPORARY/checkout
DATABASE_SECRETS=$TEMPORARY/database-secrets
SIGNING_SECRETS=$TEMPORARY/signing-secrets
EXTERNAL_ENVIRONMENT=$TEMPORARY/controller.env
POSTGRES_CONTAINER=xs-gate25-postgres-$RUN_TOKEN
SENTINEL_CONTAINER=xs-gate25-sentinel-$RUN_TOKEN
PRIME_CONTAINER=xs-gate25-prime-$RUN_TOKEN
NETWORK_CREATED=0
ORIGINAL_CAPTURED=0
FIXTURE_CAPTURED=0
FINALIZING=0

require_command() {
    command -v "$1" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$1" >&2
        exit 2
    }
}

normalize_nftables() {
    nft -j list ruleset | python3 -c '
import json
import sys


def normalize(value):
    if isinstance(value, dict):
        result = {key: normalize(item) for key, item in value.items()}
        counter = result.get("counter")
        if isinstance(counter, dict):
            counter.pop("packets", None)
            counter.pop("bytes", None)
        return result
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


json.dump(normalize(json.load(sys.stdin)), sys.stdout, sort_keys=True, separators=(",", ":"))
'
}

capture_inventory() {
    local prefix=$1
    docker ps -a --format '{{.ID}} {{.Names}} {{.Image}} {{.State}}' | sort \
        >"$EVIDENCE_DIR/$prefix-docker-containers.txt"
    docker network ls --format '{{.ID}} {{.Name}} {{.Driver}} {{.Scope}}' | sort \
        >"$EVIDENCE_DIR/$prefix-docker-networks.txt"
    docker volume ls --format '{{.Name}}' | sort \
        >"$EVIDENCE_DIR/$prefix-docker-volumes.txt"
    ip -json route show default >"$EVIDENCE_DIR/$prefix-default-routes.json"
    ip -json rule show >"$EVIDENCE_DIR/$prefix-rules.json"
    ip -o link show | awk -F': ' '{print $2}' | cut -d@ -f1 | sort \
        >"$EVIDENCE_DIR/$prefix-links.txt"
    ip netns list | awk '{print $1}' | sort >"$EVIDENCE_DIR/$prefix-netns.txt"
    normalize_nftables >"$EVIDENCE_DIR/$prefix-nftables.json"
    if command -v systemctl >/dev/null; then
        systemctl --failed --no-legend --plain 2>/dev/null | sort \
            >"$EVIDENCE_DIR/$prefix-failed-services.txt" || true
    else
        : >"$EVIDENCE_DIR/$prefix-failed-services.txt"
    fi
    if docker network inspect 1panel-network >/dev/null 2>&1; then
        docker network inspect 1panel-network | python3 -c '
import json
import sys

network = json.load(sys.stdin)[0]
print(json.dumps({
    "id": network["Id"],
    "name": network["Name"],
    "driver": network["Driver"],
    "ipam": network["IPAM"],
    "containers": network.get("Containers") or {},
}, sort_keys=True, separators=(",", ":")))
' >"$EVIDENCE_DIR/$prefix-onepanel-network.json"
    else
        printf '{"absent":true}\n' >"$EVIDENCE_DIR/$prefix-onepanel-network.json"
    fi
}

compare_inventories() {
    local before=$1 after=$2 label=$3 item status=0
    local -a items=(
        docker-containers.txt
        docker-networks.txt
        docker-volumes.txt
        default-routes.json
        rules.json
        links.txt
        netns.txt
        nftables.json
        failed-services.txt
        onepanel-network.json
    )
    : >"$EVIDENCE_DIR/$label-invariants.tsv"
    printf 'invariant\tresult\n' >>"$EVIDENCE_DIR/$label-invariants.tsv"
    for item in "${items[@]}"; do
        if cmp -s "$EVIDENCE_DIR/$before-$item" "$EVIDENCE_DIR/$after-$item"; then
            printf '%s\tPASS\n' "$item" >>"$EVIDENCE_DIR/$label-invariants.tsv"
        else
            printf '%s\tFAIL\n' "$item" >>"$EVIDENCE_DIR/$label-invariants.tsv"
            diff -u "$EVIDENCE_DIR/$before-$item" "$EVIDENCE_DIR/$after-$item" \
                >"$EVIDENCE_DIR/$label-$item.diff" || true
            status=1
        fi
    done
    return "$status"
}

run_phase() {
    local name=$1 phase_status
    shift
    printf 'running=%s\n' "$name"
    set +e
    set -o pipefail
    "$@" 2>&1 | tee "$EVIDENCE_DIR/$name.log"
    phase_status=${PIPESTATUS[0]}
    set -e
    if ((phase_status == 0)); then
        printf '%s\tPASS\n' "$name" >>"$EVIDENCE_DIR/phases.tsv"
    else
        printf '%s\tFAIL\n' "$name" >>"$EVIDENCE_DIR/phases.tsv"
    fi
    return "$phase_status"
}

cleanup_fixtures() {
    local status=0 container label members
    for container in "$SENTINEL_CONTAINER" "$POSTGRES_CONTAINER" "$PRIME_CONTAINER"; do
        if docker container inspect "$container" >/dev/null 2>&1; then
            label=$(docker container inspect "$container" \
                --format '{{index .Config.Labels "xs-nexus.qa.gate25"}}')
            if [[ $label == "$RUN_TOKEN" ]]; then
                docker rm -fv "$container" >/dev/null || status=1
            else
                printf 'refusing to remove container without exact Gate 25 label: %s\n' \
                    "$container" >&2
                status=1
            fi
        fi
    done
    if ((NETWORK_CREATED == 1)) && docker network inspect 1panel-network >/dev/null 2>&1; then
        label=$(docker network inspect 1panel-network \
            --format '{{index .Labels "xs-nexus.qa.gate25"}}')
        members=$(docker network inspect 1panel-network \
            --format '{{len .Containers}}')
        if [[ $label == "$RUN_TOKEN" && $members == 0 ]]; then
            docker network rm 1panel-network >/dev/null || status=1
        else
            printf 'refusing to remove non-empty or unowned 1panel-network fixture\n' >&2
            status=1
        fi
    fi
    return "$status"
}

finalize() {
    local status=$? cleanup_status=0 scanner_status=0 manifest_status=0 result
    ((FINALIZING == 0)) || exit "$status"
    FINALIZING=1
    trap - EXIT INT TERM
    set +e

    if ((FIXTURE_CAPTURED == 1)); then
        capture_inventory fixture-after
        compare_inventories fixture-before fixture-after fixture || status=1
    fi
    cleanup_fixtures || cleanup_status=$?
    ((cleanup_status == 0)) || status=1
    if ((ORIGINAL_CAPTURED == 1)); then
        capture_inventory final
        compare_inventories original final cleanup || status=1
    fi

    cd "$SOURCE_REPOSITORY" || status=1
    if [[ $TEMPORARY == /tmp/xs-gate25.* && -d $TEMPORARY ]]; then
        rm -rf -- "$TEMPORARY"
    else
        printf 'temporary directory safety check failed\n' >&2
        status=1
    fi

    python3 "$SOURCE_REPOSITORY/scripts/check-secrets.py" --root "$EVIDENCE_DIR" \
        >"$EVIDENCE_DIR/secret-scan.log" 2>&1 || scanner_status=$?
    ((scanner_status == 0)) || status=1
    result=FAIL
    ((status == 0)) && result=PASS
    {
        printf 'revision=%s\n' "$EXPECTED_REVISION"
        printf 'mode=ephemeral-test-signing-only\n'
        printf 'formal_signed_rc=false\n'
        printf 'independent_operator=false\n'
        printf 'production_mutation=false\n'
        printf 'status=%s\n' "$result"
        printf 'end_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$EVIDENCE_DIR/summary.txt"
    find "$EVIDENCE_DIR" -type f ! -name SHA256SUMS -print0 \
        | LC_ALL=C sort -z \
        | xargs -0 sha256sum \
        | sed "s#  $EVIDENCE_DIR/#  #" \
        >"$EVIDENCE_DIR/SHA256SUMS" || manifest_status=$?
    ((manifest_status == 0)) || status=1

    exit "$status"
}

for command in awk cargo cmp curl diff docker find git ip make nft openssl python3 \
    rustc sed sha256sum shellcheck ssh-keygen systemctl tee; do
    require_command "$command"
done
[[ $(id -u) -eq 0 ]] || {
    printf 'clean release rehearsal requires root on a disposable Linux host\n' >&2
    exit 2
}
[[ ${XS_GATE25_EPHEMERAL_HOST:-0} == 1 ]] || {
    printf 'set XS_GATE25_EPHEMERAL_HOST=1 only on a disposable approved host\n' >&2
    exit 2
}
[[ $EXPECTED_REVISION =~ ^[0-9a-f]{40}$ ]] || {
    printf 'EXPECTED_REVISION must be an exact 40-character commit\n' >&2
    exit 2
}

mkdir -p "$EVIDENCE_DIR"
EVIDENCE_DIR=$(cd "$EVIDENCE_DIR" && pwd -P)
SOURCE_REPOSITORY=$(cd "$SOURCE_REPOSITORY" && pwd -P)
[[ -z $(find "$EVIDENCE_DIR" -mindepth 1 -print -quit) ]] || {
    printf 'evidence directory must start empty\n' >&2
    exit 2
}
case "$EVIDENCE_DIR/" in
    "$SOURCE_REPOSITORY/"*)
        printf 'evidence directory must remain outside the source repository\n' >&2
        exit 2
        ;;
esac

trap finalize EXIT INT TERM
printf 'phase\tresult\n' >"$EVIDENCE_DIR/phases.tsv"
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$EVIDENCE_DIR/start-utc.txt"

source_head=$(git -c "safe.directory=$SOURCE_REPOSITORY" -C "$SOURCE_REPOSITORY" rev-parse HEAD)
[[ $source_head == "$EXPECTED_REVISION" ]] || {
    printf 'source revision mismatch: expected %s, got %s\n' \
        "$EXPECTED_REVISION" "$source_head" >&2
    exit 2
}
[[ -z $(git -c "safe.directory=$SOURCE_REPOSITORY" -C "$SOURCE_REPOSITORY" \
    status --porcelain --untracked-files=all) ]] || {
    printf 'source repository must be clean before bundling\n' >&2
    exit 2
}
[[ $(git -c "safe.directory=$SOURCE_REPOSITORY" -C "$SOURCE_REPOSITORY" \
    rev-parse --is-shallow-repository) == false ]] || {
    printf 'source repository must contain complete history for a connected release bundle\n' >&2
    exit 2
}
case "$CHECKOUT/" in
    "$EVIDENCE_DIR/"*)
        printf 'clean checkout must remain outside evidence\n' >&2
        exit 2
        ;;
esac
if docker network inspect 1panel-network >/dev/null 2>&1; then
    printf 'Gate 25 hosted rehearsal refuses an existing 1panel-network\n' >&2
    exit 2
fi

docker pull "$ALPINE_IMAGE" >"$EVIDENCE_DIR/alpine-image-pull.log" 2>&1
docker run -d --name "$PRIME_CONTAINER" \
    --publish 127.0.0.1::8080 \
    --label "xs-nexus.qa.gate25=$RUN_TOKEN" \
    "$ALPINE_IMAGE" sleep 120 >"$EVIDENCE_DIR/firewall-prime-container-id.txt"
[[ $(docker container inspect "$PRIME_CONTAINER" \
    --format '{{index .Config.Labels "xs-nexus.qa.gate25"}}') == "$RUN_TOKEN" ]]
docker rm -fv "$PRIME_CONTAINER" >/dev/null
printf 'docker_loopback_port_firewall_initialized=PASS\n' \
    >"$EVIDENCE_DIR/firewall-prime.txt"

capture_inventory original
ORIGINAL_CAPTURED=1

git -c "safe.directory=$SOURCE_REPOSITORY" -C "$SOURCE_REPOSITORY" \
    bundle create "$TEMPORARY/source.bundle" HEAD
git clone --quiet "$TEMPORARY/source.bundle" "$CHECKOUT"
git -C "$CHECKOUT" checkout --quiet --detach "$EXPECTED_REVISION"
[[ $(git -C "$CHECKOUT" rev-parse HEAD) == "$EXPECTED_REVISION" ]]
[[ $(git -C "$CHECKOUT" rev-parse 'HEAD^{tree}') == \
    $(git -c "safe.directory=$SOURCE_REPOSITORY" -C "$SOURCE_REPOSITORY" \
        rev-parse 'HEAD^{tree}') ]]
[[ -z $(git -C "$CHECKOUT" status --porcelain --untracked-files=all) ]]
git -C "$CHECKOUT" fsck --full >"$EVIDENCE_DIR/git-fsck.log" 2>&1
git -C "$CHECKOUT" ls-tree -r --full-tree HEAD \
    >"$EVIDENCE_DIR/tracked-tree.txt"

mkdir -m 0700 "$DATABASE_SECRETS" "$SIGNING_SECRETS"
ssh-keygen -q -t ed25519 -N '' -f "$SIGNING_SECRETS/tag-key"
printf 'gate25@xs-nexus.invalid %s\n' "$(cat "$SIGNING_SECRETS/tag-key.pub")" \
    >"$SIGNING_SECRETS/allowed-signers"
git -C "$CHECKOUT" config user.name 'XS Nexus Gate 25 Test'
git -C "$CHECKOUT" config user.email 'gate25@xs-nexus.invalid'
git -C "$CHECKOUT" config gpg.format ssh
git -C "$CHECKOUT" config user.signingkey "$SIGNING_SECRETS/tag-key"
git -C "$CHECKOUT" config gpg.ssh.allowedSignersFile "$SIGNING_SECRETS/allowed-signers"
test_tag=gate25-test-${EXPECTED_REVISION:0:12}
git -C "$CHECKOUT" tag -s "$test_tag" "$EXPECTED_REVISION" \
    -m 'ephemeral Gate 25 rehearsal tag'
git -C "$CHECKOUT" tag -v "$test_tag" >"$EVIDENCE_DIR/test-tag-verification.txt" 2>&1
{
    printf 'tag=%s\n' "$test_tag"
    printf 'revision=%s\n' "$EXPECTED_REVISION"
    printf 'signing_mode=ephemeral-test-only\n'
    printf 'formal_rc=false\n'
} >"$EVIDENCE_DIR/source-identity.txt"

cd "$CHECKOUT"
run_phase source-secret-scan python3 scripts/check-secrets.py --root .
run_phase release-provenance make test-release-provenance
run_phase image-reproducibility make test-image-reproducibility \
    EVIDENCE_DIR="$EVIDENCE_DIR/image-reproducibility"

docker pull "$POSTGRES_IMAGE" >"$EVIDENCE_DIR/postgres-image-pull.log" 2>&1
docker network create --driver bridge --subnet 172.18.0.0/16 \
    --label "xs-nexus.qa.gate25=$RUN_TOKEN" 1panel-network \
    >"$EVIDENCE_DIR/fixture-network-id.txt"
NETWORK_CREATED=1
docker run -d --name "$SENTINEL_CONTAINER" --network 1panel-network \
    --label "xs-nexus.qa.gate25=$RUN_TOKEN" \
    "$ALPINE_IMAGE" sleep 7200 >"$EVIDENCE_DIR/sentinel-container-id.txt"

openssl rand -hex 32 >"$DATABASE_SECRETS/bootstrap-password"
openssl rand -hex 32 >"$DATABASE_SECRETS/app-password"
openssl rand -hex 32 >"$DATABASE_SECRETS/migrator-password"
chmod 0400 "$DATABASE_SECRETS"/*
postgres_uid=$(docker run --rm --entrypoint id "$POSTGRES_IMAGE" -u postgres)
chown -R "$postgres_uid:$postgres_uid" "$DATABASE_SECRETS"
docker run -d --name "$POSTGRES_CONTAINER" --network 1panel-network \
    --label "xs-nexus.qa.gate25=$RUN_TOKEN" \
    --mount "type=bind,src=$DATABASE_SECRETS,dst=/run/test-secrets,readonly" \
    --mount "type=bind,src=$CHECKOUT,dst=/workspace,readonly" \
    -e POSTGRES_USER=gate25_bootstrap \
    -e POSTGRES_DB=gate25 \
    -e POSTGRES_PASSWORD_FILE=/run/test-secrets/bootstrap-password \
    "$POSTGRES_IMAGE" >"$EVIDENCE_DIR/postgres-container-id.txt"
for _attempt in $(seq 1 60); do
    if docker exec "$POSTGRES_CONTAINER" pg_isready -q -U gate25_bootstrap -d gate25; then
        break
    fi
    sleep 1
done
docker exec "$POSTGRES_CONTAINER" pg_isready -q -U gate25_bootstrap -d gate25
docker exec -u postgres "$POSTGRES_CONTAINER" sh -eu -c '
    export PGPASSWORD=$(cat /run/test-secrets/bootstrap-password)
    export XS_DATABASE_APP_PASSWORD=$(cat /run/test-secrets/app-password)
    export XS_DATABASE_MIGRATOR_PASSWORD=$(cat /run/test-secrets/migrator-password)
    psql -q -U gate25_bootstrap -d gate25 \
        -v schema=gate25_role_seed -v owner_role=gate25_owner \
        -v app_role=gate25_app -v migrator_role=gate25_migrator \
        -f /workspace/deploy/docker/postgres-role-hardening.sql
' >"$EVIDENCE_DIR/postgres-role-hardening.log" 2>&1

database_host=$(docker inspect "$POSTGRES_CONTAINER" \
    --format '{{(index .NetworkSettings.Networks "1panel-network").IPAddress}}')
[[ $database_host =~ ^172\.18\.[0-9]{1,3}\.[0-9]{1,3}$ ]]
python3 - "$DATABASE_SECRETS" "$POSTGRES_CONTAINER" "$database_host" \
    "$EXTERNAL_ENVIRONMENT" <<'PY'
import sys
from pathlib import Path
from urllib.parse import quote, urlunsplit

secrets = Path(sys.argv[1])
container = sys.argv[2]
database_host = sys.argv[3]
output = Path(sys.argv[4])


def value(name):
    return (secrets / name).read_text(encoding="utf-8").strip()


def database_url(username, password, host, port):
    userinfo = f"{username}:{quote(password, safe='')}"
    return urlunsplit(("postgresql", f"{userinfo}@{host}:{port}", "/gate25", "", ""))


bootstrap_url = database_url(
    "gate25_bootstrap", value("bootstrap-password"), database_host, "5432"
)
app_url = database_url("gate25_app", value("app-password"), container, "5432")
migrator_url = database_url(
    "gate25_migrator", value("migrator-password"), container, "5432"
)
output.write_text(
    "\n".join([
        f"DATABASE_URL={app_url}",
        f"MIGRATION_DATABASE_URL={migrator_url}",
        f"HOST_DATABASE_URL={bootstrap_url}",
        "DATABASE_OWNER_ROLE=gate25_owner",
        "",
    ]),
    encoding="utf-8",
)
(secrets / "host-database-url").write_text(
    bootstrap_url,
    encoding="utf-8",
)
PY
chmod 0600 "$EXTERNAL_ENVIRONMENT" "$DATABASE_SECRETS/host-database-url"
export XS_TEST_DATABASE_URL
XS_TEST_DATABASE_URL=$(cat "$DATABASE_SECRETS/host-database-url")
export XS_TEST_EXTERNAL_ENVIRONMENT=$EXTERNAL_ENVIRONMENT

capture_inventory fixture-before
FIXTURE_CAPTURED=1

run_phase workspace-build cargo build --workspace
run_phase direct-acl make test-agent-acl
run_phase relay-recovery make test-agent-relay
run_phase subnet-routing make test-agent-subnet-route
run_phase update-supply-chain env \
    EVIDENCE_DIR="$EVIDENCE_DIR/update-supply-chain" make test-update-supply-chain
run_phase docker-lifecycle-install env \
    XS_TEST_EXTERNAL_ENVIRONMENT="$EXTERNAL_ENVIRONMENT" make test-docker-deployment
run_phase docker-lifecycle-reinstall env \
    XS_TEST_EXTERNAL_ENVIRONMENT="$EXTERNAL_ENVIRONMENT" make test-docker-deployment

[[ -z $(git -C "$CHECKOUT" status --porcelain --untracked-files=all) ]]
printf 'Clean release rehearsal repository submatrix passed\n'
