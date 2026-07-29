#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

generated="$(mktemp)"
trap 'rm -f "$generated"' EXIT

cargo run --quiet -p xs-protocol --example generate_credential_vector >"$generated"
if ! cmp --silent "$generated" tests/vectors/xsp1/credential-v1.json; then
    diff -u tests/vectors/xsp1/credential-v1.json "$generated" || true
    printf 'credential vector differs from the checked-in file\n' >&2
    exit 1
fi

printf 'XSP/1 credential vector generation passed\n'
