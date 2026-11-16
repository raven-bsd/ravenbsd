#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the aarch64 pi4-root probe and wrap the B63 UFS2 golden image in the
# deterministic one-partition GPT disk the EMMC2 fake card serves.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-root
mkdir -p "$out"

gzip -dc tests/ufs-vectors/golden/ufs2-b4096.img.gz > "$out/ufs.img"
cargo run -q -p raven-k54-mkimage -- "$out/ufs.img" "$out/disk.img"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-k54-probe --target aarch64-unknown-none-softfloat

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/aarch64-unknown-none-softfloat/debug/raven-k54-probe" "$out/probe"
