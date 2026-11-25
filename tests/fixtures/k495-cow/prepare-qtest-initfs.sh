#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S164 test-only: build the Rust C13 fixture and a libc-test-profile initfs whose one-shot
# /bin/libc-test service is that fixture. The qtest installs the initfs over the libc-test
# profile; the fixture runs the private fork COW rows, the shared-object rows and fork+exec and
# exits. The published K495 C blob is not built or modified by this image.
#
# Usage: sh tests/fixtures/k495-cow/prepare-qtest-initfs.sh
# Output: build/qtest/cow-visibility-initfs.img

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/qtest
mkdir -p "$out"

sh tests/libc/target/build.sh termios > "$out/cow-libc-build.log" 2>&1

manifest=tests/fixtures/k495-cow/memobj/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-$root/target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-k495-cow-fixture --target x86_64-unknown-none --release \
    > "$out/cow-fixture-build.log" 2>&1
cp "$target_dir/x86_64-unknown-none/release/raven-k495-cow-fixture" "$out/k495-cow-fixture"
rm -f tests/fixtures/k495-cow/memobj/Cargo.lock

if [ ! -x target/debug/raven-b70-overlay ]; then
    cargo build -q -p raven-b70-overlay > "$out/cow-overlay-build.log" 2>&1
fi

cargo xtask initfs --arch x86_64 --profile libc-test > "$out/cow-initfs-build.log" 2>&1

target/debug/raven-b70-overlay build/x86_64/initfs.img "$out/cow-visibility-initfs.img" \
    "/bin/libc-test=$out/k495-cow-fixture:0755" \
    > "$out/cow-overlay.log" 2>&1

echo "cow-visibility: initfs $out/cow-visibility-initfs.img"
