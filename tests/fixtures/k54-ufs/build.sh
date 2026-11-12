#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the fsd-ufs qtest client and wrap the B63 UFS2 golden image in a
# one-partition GPT disk for the K14 RAM block device.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k54-ufs
mkdir -p "$out"

gzip -dc tests/ufs-vectors/golden/ufs2-b4096.img.gz > "$out/ufs.img"
cargo run -q -p raven-k54-mkimage -- "$out/ufs.img" "$out/disk.img"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-k54-probe --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-k54-probe" "$out/probe"
