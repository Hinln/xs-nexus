#!/usr/bin/env bash
set -euo pipefail

readonly RSA_ADVISORY="RUSTSEC-2023-0071"
readonly PASTE_ADVISORY="RUSTSEC-2024-0436"
readonly PASTE_REVIEW_DEADLINE="2026-08-31"

if [[ "$(date -u +%F)" > "${PASTE_REVIEW_DEADLINE}" ]]; then
    printf 'paste advisory exception expired on %s\n' "${PASTE_REVIEW_DEADLINE}" >&2
    exit 1
fi

cargo metadata --locked --no-deps --format-version 1 >/dev/null

if rsa_tree="$(cargo tree --workspace --all-features --target all -i rsa)" && [[ -n "${rsa_tree}" ]]; then
    printf 'rsa advisory dependency is reachable:\n%s\n' "${rsa_tree}" >&2
    exit 1
fi

paste_tree="$(cargo tree --workspace --all-features --target all -i paste)"
if ! grep -Fq 'rtnetlink' <<<"${paste_tree}"; then
    printf 'paste dependency path changed and requires review:\n%s\n' "${paste_tree}" >&2
    exit 1
fi

cargo-audit audit --ignore "${RSA_ADVISORY}" --ignore "${PASTE_ADVISORY}"
cargo-deny --all-features check advisories bans sources
