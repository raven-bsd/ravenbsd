#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P5c/P5c2 record-lock fixture (raw syscall and libc fcntl) and pack its initfs.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/record-lock
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
archive=$root/build/x86_64/libc.a

if [ ! -f "$archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc" -name '*.rs' -newer "$archive" -print -quit)" ]; then
    cargo xtask libc-pack --arch x86_64
fi

RUSTFLAGS="$rustflags" \
    cargo build -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    --target "$target" --release

RUSTFLAGS="$rustflags -C link-arg=-L$root/build/x86_64 -C link-arg=-lc" \
    cargo build -p raven-record-lock \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/record-lock-fixture" \
    "/sbin/record-lock-child=$target_dir/$target/release/record-lock-child" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd"
