#!/usr/bin/env bash
set -euo pipefail

components=(agent cli controller relay)
for component in "${components[@]}"; do
  package="xs-${component}"
  output="$(cargo run --quiet --package "${package}")"
  expected="${package} status=baseline-ready version=0.1.0"
  if [[ "${output}" != "${expected}" ]]; then
    printf 'unexpected output for %s: %s\n' "${package}" "${output}" >&2
    exit 1
  fi
done

printf 'component integration checks passed\n'
