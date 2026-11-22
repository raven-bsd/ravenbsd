#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K524 AT_RANDOM/GETRANDOM fixture and pack its initfs.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/random
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-random-pool -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-random-pool" \
    "/child=$target_dir/$target/release/raven-random-pool" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd"

printf 'random\n' > "$out/stamp"
printf 'random fixture: staged %s\n' "$out/initfs.img"
