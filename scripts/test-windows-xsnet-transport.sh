#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
RUST_SOURCE="$(rustc --print sysroot)/lib/rustlib/src/rust/library"
TARGET_DIR=$(mktemp -d /tmp/xs-nexus-windows-transport.XXXXXX)
trap 'rm -rf -- "$TARGET_DIR"' EXIT INT TERM

[[ -d "$RUST_SOURCE/core" && -d "$RUST_SOURCE/alloc" ]] || {
    printf 'matching Rust core and alloc source is required\n' >&2
    exit 2
}

cd "$ROOT_DIR"
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$TARGET_DIR"
export RUSTC_BOOTSTRAP=1

cargo check --locked \
    --target x86_64-pc-windows-msvc \
    -Z build-std=core,alloc,panic_abort \
    -p xs-windows-transport
cargo clippy --locked \
    --target x86_64-pc-windows-msvc \
    -Z build-std=core,alloc,panic_abort \
    -p xs-windows-transport \
    -- -D warnings

printf 'xsnet Win32 transport MSVC target check passed\n'
