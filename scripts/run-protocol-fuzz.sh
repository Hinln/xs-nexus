#!/usr/bin/env bash
set -Eeuo pipefail
umask 027

readonly NIGHTLY_TOOLCHAIN="nightly-2026-08-01"
readonly CARGO_FUZZ_VERSION="0.13.2"
readonly FUZZ_SECONDS="${FUZZ_SECONDS:-180}"
readonly FUZZ_SANITIZER="${FUZZ_SANITIZER:-address}"
readonly EVIDENCE_INPUT="${1:?usage: run-protocol-fuzz.sh EVIDENCE_DIR}"
readonly TARGETS=(credential data discovery handshake relay session_state)

REPOSITORY_ROOT="$(git rev-parse --show-toplevel)"
readonly REPOSITORY_ROOT
cd "${REPOSITORY_ROOT}"
EVIDENCE_DIR="$(realpath -m -- "${EVIDENCE_INPUT}")"
readonly EVIDENCE_DIR

if [[ ! "${FUZZ_SECONDS}" =~ ^[1-9][0-9]*$ ]]; then
    printf 'FUZZ_SECONDS must be a positive integer\n' >&2
    exit 2
fi
if ((FUZZ_SECONDS < 180)); then
    printf 'FUZZ_SECONDS must be at least 180\n' >&2
    exit 2
fi
if [[ "${FUZZ_SANITIZER}" != "address" ]]; then
    printf 'FUZZ_SANITIZER must be address\n' >&2
    exit 2
fi
if [[ -e "${EVIDENCE_DIR}" ]]; then
    printf 'evidence destination must not already exist\n' >&2
    exit 2
fi

mkdir -p "${EVIDENCE_DIR}"
STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
readonly STARTED_AT

finalize_evidence() {
    local exit_code=$?
    local status="FAIL"
    trap - EXIT
    set +e
    if ((exit_code == 0)); then
        status="PASS"
    fi
    {
        printf 'status=%s\n' "${status}"
        printf 'revision=%s\n' "$(git rev-parse HEAD 2>/dev/null || printf unknown)"
        printf 'started_at=%s\n' "${STARTED_AT}"
        printf 'completed_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'sanitizer=%s\n' "${FUZZ_SANITIZER}"
        printf 'seconds_per_target=%s\n' "${FUZZ_SECONDS}"
        printf 'targets=%s\n' "${TARGETS[*]}"
    } >"${EVIDENCE_DIR}/summary.txt"
    if ! (
        cd "${EVIDENCE_DIR}"
        find . -type f ! -name SHA256SUMS -print0 \
            | LC_ALL=C sort -z \
            | xargs -0 -r sha256sum
    ) >"${EVIDENCE_DIR}/SHA256SUMS"; then
        exit_code=1
        sed -i 's/^status=.*/status=FAIL/' "${EVIDENCE_DIR}/summary.txt"
    fi
    exit "${exit_code}"
}
trap finalize_evidence EXIT

git rev-parse HEAD >"${EVIDENCE_DIR}/revision.txt"
git status --porcelain=v1 --untracked-files=no >"${EVIDENCE_DIR}/tracked-status.txt"
if [[ -s "${EVIDENCE_DIR}/tracked-status.txt" ]]; then
    printf 'tracked worktree must be clean\n' >&2
    exit 2
fi

rustc "+${NIGHTLY_TOOLCHAIN}" -vV | tee "${EVIDENCE_DIR}/rustc-version.txt"
cargo "+${NIGHTLY_TOOLCHAIN}" --version | tee "${EVIDENCE_DIR}/cargo-version.txt"
CARGO_FUZZ_ACTUAL_VERSION="$(cargo-fuzz --version)"
readonly CARGO_FUZZ_ACTUAL_VERSION
printf '%s\n' "${CARGO_FUZZ_ACTUAL_VERSION}" | tee "${EVIDENCE_DIR}/cargo-fuzz-version.txt"
if [[ "${CARGO_FUZZ_ACTUAL_VERSION}" != "cargo-fuzz ${CARGO_FUZZ_VERSION}" ]]; then
    printf 'cargo-fuzz version must be %s\n' "${CARGO_FUZZ_VERSION}" >&2
    exit 2
fi
command -v cargo-fuzz >"${EVIDENCE_DIR}/cargo-fuzz-path.txt"
sha256sum "$(command -v cargo-fuzz)" >"${EVIDENCE_DIR}/cargo-fuzz-binary.sha256"

{
    printf 'nightly_toolchain=%s\n' "${NIGHTLY_TOOLCHAIN}"
    printf 'cargo_fuzz_version=%s\n' "${CARGO_FUZZ_VERSION}"
    printf 'sanitizer=%s\n' "${FUZZ_SANITIZER}"
    printf 'seconds_per_target=%s\n' "${FUZZ_SECONDS}"
    printf 'targets=%s\n' "${TARGETS[*]}"
} >"${EVIDENCE_DIR}/configuration.txt"

sha256sum \
    fuzz/Cargo.toml \
    fuzz/Cargo.lock \
    fuzz/fuzz_targets/*.rs \
    scripts/run-protocol-fuzz.sh \
    scripts/validate-protocol-fuzz.py \
    >"${EVIDENCE_DIR}/harness-sources.sha256"

cargo "+${NIGHTLY_TOOLCHAIN}" fmt \
    --manifest-path fuzz/Cargo.toml \
    -- \
    --check \
    2>&1 | tee "${EVIDENCE_DIR}/format.log"

cargo "+${NIGHTLY_TOOLCHAIN}" clippy \
    --locked \
    --manifest-path fuzz/Cargo.toml \
    --all-targets \
    -- \
    -D warnings \
    2>&1 | tee "${EVIDENCE_DIR}/clippy.log"

cargo "+${NIGHTLY_TOOLCHAIN}" fuzz build \
    --sanitizer "${FUZZ_SANITIZER}" \
    2>&1 | tee "${EVIDENCE_DIR}/build.log"

for target in "${TARGETS[@]}"; do
    source_corpus="fuzz/corpus/${target}"
    if [[ ! -d "${source_corpus}" ]] \
        || [[ -z "$(find "${source_corpus}" -type f -print -quit)" ]]; then
        printf 'missing seed corpus for %s\n' "${target}" >&2
        exit 2
    fi
    target_dir="${EVIDENCE_DIR}/${target}"
    mkdir -p "${target_dir}/corpus" "${target_dir}/artifacts"
    cp -a "${source_corpus}/." "${target_dir}/corpus/"
    cargo "+${NIGHTLY_TOOLCHAIN}" fuzz run \
        --sanitizer "${FUZZ_SANITIZER}" \
        "${target}" "${target_dir}/corpus" -- \
        -artifact_prefix="${target_dir}/artifacts/" \
        -max_total_time="${FUZZ_SECONDS}" \
        -print_final_stats=1 \
        -timeout=10 2>&1 | tee "${target_dir}/fuzz.log"
    if [[ -n "$(find "${target_dir}/artifacts" -type f -print -quit)" ]]; then
        printf 'crash artifacts detected for %s\n' "${target}" >&2
        exit 1
    fi
    printf 'status=PASS\n' >"${target_dir}/status.txt"
done
