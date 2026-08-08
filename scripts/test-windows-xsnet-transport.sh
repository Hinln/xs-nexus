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
RUST_TARGET=x86_64-pc-windows-msvc
RUST_TARGET_LIB="$("$RUSTC_BIN" --print sysroot)/lib/rustlib/$RUST_TARGET/lib"
TARGET_DIR=$(mktemp -d /tmp/xs-nexus-windows-transport.XXXXXX)
trap 'rm -rf -- "$TARGET_DIR"' EXIT INT TERM

[[ -d "$RUST_TARGET_LIB" ]] || {
    printf 'matching %s Rust target is required\n' "$RUST_TARGET" >&2
    exit 2
}

cd "$ROOT_DIR"
export CARGO_NET_OFFLINE=true
"$CARGO_BIN" test --locked -p xs-agent --lib windows_xsnet::tests

export CARGO_TARGET_DIR="$TARGET_DIR"

"$CARGO_BIN" check --locked \
    --target "$RUST_TARGET" \
    -p xs-windows-transport
"$CLIPPY_BIN" clippy --locked \
    --target "$RUST_TARGET" \
    -p xs-windows-transport \
    -- -D warnings

printf 'xsnet Agent session and Win32 transport MSVC target checks passed\n'
