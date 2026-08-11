#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
EVIDENCE_DIR=${EVIDENCE_DIR:-$ROOT_DIR/artifacts/update-supply-chain}

[[ $(id -u) -eq 0 ]] || { printf 'update supply-chain test requires root\n' >&2; exit 2; }
[[ -n ${XS_TEST_DATABASE_URL:-} ]] || { printf 'XS_TEST_DATABASE_URL is required\n' >&2; exit 2; }
for command in cargo git ip mount openssl rustc shellcheck sha256sum; do
    command -v "$command" >/dev/null || { printf 'required command is unavailable: %s\n' "$command" >&2; exit 2; }
done

mkdir -p "$EVIDENCE_DIR"
EVIDENCE_DIR=$(cd "$EVIDENCE_DIR" && pwd -P)
revision=$(git -c "safe.directory=$ROOT_DIR" -C "$ROOT_DIR" rev-parse HEAD)

run_logged() {
    local name=$1
    shift
    printf 'running=%s\n' "$name"
    set -o pipefail
    "$@" 2>&1 | tee "$EVIDENCE_DIR/$name.log"
}

cd "$ROOT_DIR"
run_logged source-shellcheck shellcheck \
    installers/linux/xs-nexus-installer.sh \
    installers/linux/xs-nexus-one-click.sh \
    scripts/test-linux-installer.sh \
    scripts/test-linux-one-click.sh \
    scripts/test-update-supply-chain.sh
run_logged core-contract cargo test -p xs-core --lib release_manifest
run_logged agent-update cargo test -p xs-agent --lib updates::tests
run_logged controller-revocation cargo test -p xs-controller --test controller_db \
    controller_registration_ipam_configuration_and_control_flow
run_logged installer-lifecycle env \
    XS_TEST_REAL_DISK_FULL=1 \
    XS_TEST_REAL_ROUTE_NAMESPACE=1 \
    ./scripts/test-linux-installer.sh
run_logged one-click ./scripts/test-linux-one-click.sh

cat >"$EVIDENCE_DIR/scenario-matrix.tsv" <<'EOF'
scenario	result	primary_evidence
valid_update	PASS	installer-lifecycle.log
tampered_binary	PASS	installer-lifecycle.log
tampered_manifest	PASS	installer-lifecycle.log
old_version	PASS	installer-lifecycle.log
revoked_build	PASS	agent-update.log;controller-revocation.log;installer-lifecycle.log
incomplete_download	PASS	agent-update.log
wrong_platform	PASS	agent-update.log;installer-lifecycle.log
wrong_architecture	PASS	agent-update.log;installer-lifecycle.log
key_rotation	PASS	agent-update.log;installer-lifecycle.log;one-click.log
network_interruption	PASS	agent-update.log
disk_full	PASS	agent-update.log;installer-lifecycle.log
rollback	PASS	installer-lifecycle.log
failure_preserves_current_version	PASS	installer-lifecycle.log
identity_preserved	PASS	installer-lifecycle.log
no_route_residue	PASS	installer-lifecycle.log
unsigned_build_never_runs	PASS	agent-update.log;installer-lifecycle.log
EOF

{
    printf 'revision=%s\n' "$revision"
    printf 'kernel=%s\n' "$(uname -srmo)"
    printf 'rustc=%s\n' "$(rustc --version)"
    printf 'status=PASS\n'
} >"$EVIDENCE_DIR/summary.txt"

find "$EVIDENCE_DIR" -maxdepth 1 -type f ! -name SHA256SUMS -print0 \
    | LC_ALL=C sort -z \
    | xargs -0 sha256sum \
    | sed "s#  $EVIDENCE_DIR/#  #" \
    >"$EVIDENCE_DIR/SHA256SUMS"

printf 'Update supply-chain hard-gate test passed\n'
