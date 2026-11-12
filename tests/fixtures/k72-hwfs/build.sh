#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K72 probe (release, so the root UFS stays inside the 1 MiB scan prefix), stage the
# small root tree, format the UFS2 root, and wrap it in a standard GPT disk.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k72-hwfs
staging=$out/staging
rm -rf "$out"
mkdir -p "$staging/sbin" "$staging/etc/services.d" "$staging/tmp"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-k72-probe --release --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/release/raven-k72-probe" "$staging/sbin/k72-probe"
cp tests/fixtures/k72-hwfs/root/etc/services.d/k72-probe.toml "$staging/etc/services.d/k72-probe.toml"
cp tests/fixtures/k72-hwfs/root/etc/rc.conf "$staging/etc/rc.conf"

cargo run -q -p ufs-mkfs -- \
    -O 2 -b 4096 -f 512 -s 960k -T 0 -d "$staging" "$out/root.ufs"
cargo run -q -p raven-k72-mkdisk -- mkdisk "$out/root.ufs" "$out/disk.img"
