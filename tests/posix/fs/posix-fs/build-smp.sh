#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S85e VFS SMP stress fixture and pack build/fixtures/posix-fs-smp/initfs.img.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/posix-fs-smp
out_start=build/fixtures/posix-fs-start
mkdir -p "$out" "$out_start"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-posix-fs \
    --bin raven-posix-fs-smp --bin raven-posix-fs-start --target "$target" --release
RUSTFLAGS="$rustflags" \
    cargo build -p raven-initfsd -p raven-ramfsd -p raven-vfsd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-posix-fs-smp" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out_start/initfs.img" \
    "/init=$target_dir/$target/release/raven-posix-fs-start" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd"
