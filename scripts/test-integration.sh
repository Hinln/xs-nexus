#!/usr/bin/env bash
set -euo pipefail

test_commit=0123456789abcdef0123456789abcdef01234567
agent_output=$(XS_BUILD_GIT_COMMIT="$test_commit" XS_BUILD_DATE_EPOCH=1700000000 cargo run --quiet --package xs-agent -- --version)
[[ "$agent_output" == "xs-agent version=0.1.0 commit=$test_commit protocol=XSP/1" ]]

cli_output=$(XS_BUILD_GIT_COMMIT="$test_commit" XS_BUILD_DATE_EPOCH=1700000000 cargo run --quiet --package xs-cli --bin xs -- --version)
[[ "$cli_output" == "xs-cli version=0.1.0 commit=$test_commit protocol=XSP/1" ]]

relay_output=$(XS_BUILD_GIT_COMMIT="$test_commit" XS_BUILD_DATE_EPOCH=1700000000 cargo run --quiet --package xs-relay -- --version)
[[ "$relay_output" == "xs-relay version=0.1.0 commit=$test_commit protocol=XSP/1" ]]

printf 'non-database component integration checks passed\n'
