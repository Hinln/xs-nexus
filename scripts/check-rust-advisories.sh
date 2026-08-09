#!/usr/bin/env bash
set -euo pipefail

readonly PASTE_ADVISORY="RUSTSEC-2024-0436"
readonly PASTE_REVIEW_DEADLINE="2026-08-31"

if [[ "$(date -u +%F)" > "${PASTE_REVIEW_DEADLINE}" ]]; then
    printf 'paste advisory exception expired on %s\n' "${PASTE_REVIEW_DEADLINE}" >&2
    exit 1
fi

cargo metadata --locked --no-deps --format-version 1 >/dev/null

if grep -Fq 'name = "rsa"' Cargo.lock; then
    printf 'rsa is present in Cargo.lock; RUSTSEC-2023-0071 must not be ignored\n' >&2
    exit 1
fi

paste_tree="$(cargo tree --workspace --all-features --target all -i paste)"
if ! grep -Fq 'rtnetlink' <<<"${paste_tree}"; then
    printf '%s dependency path changed and requires review:\n%s\n' "${PASTE_ADVISORY}" "${paste_tree}" >&2
    exit 1
fi

cargo-audit audit
cargo-deny --all-features check
