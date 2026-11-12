#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K15 probe, then create the 1 GiB sparse GPT disk the qtest attaches to QEMU.
# The driver itself is a workspace package built by the profile's programs list.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k15-blk
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-k15-probe --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-k15-probe" "$out/probe"

cargo run -q -p raven-k15-mkdisk -- "$out/disk.img"
