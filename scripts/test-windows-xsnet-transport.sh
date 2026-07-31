#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
TOOL_DIR=$(dirname "$(command -v cargo)")
CARGO_BIN="$TOOL_DIR/cargo"
RUSTC_BIN="$TOOL_DIR/rustc"
CLIPPY_BIN="$TOOL_DIR/cargo-clippy"
[[ -x "$CARGO_BIN" && -x "$RUSTC_BIN" && -x "$CLIPPY_BIN" ]] || {
    printf 'matching cargo, rustc, and cargo-clippy are required\n' >&2
    exit 2
}
RUST_SOURCE="$("$RUSTC_BIN" --print sysroot)/lib/rustlib/src/rust/library"
TARGET_DIR=$(mktemp -d /tmp/xs-nexus-windows-transport.XXXXXX)
trap 'rm -rf -- "$TARGET_DIR"' EXIT INT TERM

[[ -d "$RUST_SOURCE/core" && -d "$RUST_SOURCE/alloc" ]] || {
    printf 'matching Rust core and alloc source is required\n' >&2
    exit 2
}

cd "$ROOT_DIR"
export CARGO_NET_OFFLINE=true
"$CARGO_BIN" test --locked -p xs-agent --lib windows_xsnet::tests

export CARGO_TARGET_DIR="$TARGET_DIR"
export RUSTC_BOOTSTRAP=1

"$CARGO_BIN" check --locked \
    --target x86_64-pc-windows-msvc \
    -Z build-std=core,alloc,panic_abort \
    -p xs-windows-transport
"$CLIPPY_BIN" clippy --locked \
    --target x86_64-pc-windows-msvc \
    -Z build-std=core,alloc,panic_abort \
    -p xs-windows-transport \
    -- -D warnings

printf 'xsnet Agent session and Win32 transport MSVC target checks passed\n'
