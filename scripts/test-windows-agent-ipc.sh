#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

cd "$ROOT_DIR"
export CARGO_NET_OFFLINE=true

./scripts/validate-windows-agent-ipc.py
cargo test --locked -p xs-windows-local-ipc
cargo test --locked -p xs-agent --test ipc

cross_cargo=cargo
if [[ -x "$HOME/.cargo/bin/rustup" ]] &&
    "$HOME/.cargo/bin/rustup" target list --installed | grep -Fxq x86_64-pc-windows-msvc; then
    cross_cargo="$HOME/.cargo/bin/cargo"
fi
"$cross_cargo" check --locked --target x86_64-pc-windows-msvc -p xs-windows-local-ipc
"$cross_cargo" clippy --locked --target x86_64-pc-windows-msvc -p xs-windows-local-ipc -- -D warnings

printf 'Windows Agent private local IPC checks passed\n'
