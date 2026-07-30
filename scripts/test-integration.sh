#!/usr/bin/env bash
set -euo pipefail

agent_output=$(cargo run --quiet --package xs-agent -- --version)
[[ "$agent_output" == 'xs-agent 0.1.0' ]]

cli_output=$(cargo run --quiet --package xs-cli --bin xs -- --version)
[[ "$cli_output" == 'xs 0.1.0' ]]

relay_output=$(cargo run --quiet --package xs-relay -- --version)
[[ "$relay_output" == 'xs-relay 0.1.0' ]]

printf 'non-database component integration checks passed\n'
