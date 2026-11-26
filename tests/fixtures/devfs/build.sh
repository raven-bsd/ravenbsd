#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K537 devfs-node fixture and pack build/fixtures/devfs/initfs.img.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/devfs
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-devfs-nodes -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-devfs-nodes" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd"

printf 'devfs-nodes\n' > "$out/stamp"
printf 'devfs-nodes fixture: staged %s\n' "$out/initfs.img"
