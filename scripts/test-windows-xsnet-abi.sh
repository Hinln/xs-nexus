#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
RELEASE_BUILD="$ROOT_DIR/target/xsnet-abi"
SANITIZER_BUILD="$ROOT_DIR/target/xsnet-abi-sanitizer"

rm -rf -- "$RELEASE_BUILD" "$SANITIZER_BUILD"

cmake \
    -S "$ROOT_DIR/drivers/windows-xsnet" \
    -B "$RELEASE_BUILD" \
    -G Ninja \
    -DCMAKE_C_COMPILER=clang \
    -DCMAKE_BUILD_TYPE=Release
cmake --build "$RELEASE_BUILD"
ctest --test-dir "$RELEASE_BUILD" --output-on-failure

cmake \
    -S "$ROOT_DIR/drivers/windows-xsnet" \
    -B "$SANITIZER_BUILD" \
    -G Ninja \
    -DCMAKE_C_COMPILER=gcc \
    -DCMAKE_BUILD_TYPE=Debug \
    -DCMAKE_C_FLAGS=-fsanitize=address,undefined\ -fno-omit-frame-pointer
cmake --build "$SANITIZER_BUILD"
ctest --test-dir "$SANITIZER_BUILD" --output-on-failure

printf 'xsnet portable ABI release and sanitizer tests passed\n'
