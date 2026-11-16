#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the pi4-emmc2 probe and install it where the profile expects a generated file.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-emmc2
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-pi4-emmc2-probe --target aarch64-unknown-none-softfloat

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/aarch64-unknown-none-softfloat/debug/raven-pi4-emmc2-probe" "$out/probe"
