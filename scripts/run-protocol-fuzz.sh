#!/usr/bin/env bash
set -Eeuo pipefail

readonly NIGHTLY_TOOLCHAIN="nightly-2026-08-01"
readonly FUZZ_SECONDS="${FUZZ_SECONDS:-60}"
readonly EVIDENCE_DIR="${1:?usage: run-protocol-fuzz.sh EVIDENCE_DIR}"
readonly TARGETS=(credential data discovery handshake relay)

if [[ ! "${FUZZ_SECONDS}" =~ ^[1-9][0-9]*$ ]]; then
    printf 'FUZZ_SECONDS must be a positive integer\n' >&2
    exit 2
fi

mkdir -p "${EVIDENCE_DIR}"
rustc "+${NIGHTLY_TOOLCHAIN}" --version | tee "${EVIDENCE_DIR}/rustc-version.txt"
cargo-fuzz --version | tee "${EVIDENCE_DIR}/cargo-fuzz-version.txt"

for target in "${TARGETS[@]}"; do
    target_dir="${EVIDENCE_DIR}/${target}"
    mkdir -p "${target_dir}/corpus" "${target_dir}/artifacts"
    cp -a "fuzz/corpus/${target}/." "${target_dir}/corpus/"
    cargo "+${NIGHTLY_TOOLCHAIN}" fuzz run "${target}" "${target_dir}/corpus" -- \
        -artifact_prefix="${target_dir}/artifacts/" \
        -max_total_time="${FUZZ_SECONDS}" \
        -print_final_stats=1 \
        -timeout=10 2>&1 | tee "${target_dir}/fuzz.log"
done
