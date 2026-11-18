#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the read-write root probe and reuse the read-only fixture image/disk.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

sh tests/fixtures/pi4-root/build.sh

out=build/fixtures/pi4-root
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p pi4-root-rw-probe --target aarch64-unknown-none-softfloat

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/aarch64-unknown-none-softfloat/debug/pi4-root-rw-probe" "$out/rw-probe"
