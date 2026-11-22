#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K456 syscall robustness fixture and pack build/syscall-fuzz/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target_triple=x86_64-unknown-none
        linker=$root/tests/smp/syscall-fuzz/linker.ld
        out=build/syscall-fuzz
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker"
        ;;
    aarch64)
        target_triple=aarch64-unknown-none-softfloat
        linker=$root/tests/smp/syscall-fuzz/linker-aarch64.ld
        out=build/syscall-fuzz/aarch64
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$linker"
        ;;
    *)
        printf 'skipped: syscall-fuzz supports x86_64 and aarch64 (got %s)\n' "$arch"
        exit 77
        ;;
esac

target_dir=${CARGO_TARGET_DIR:-target}
binary=$target_dir/$target_triple/release/raven-syscall-fuzz-init
mkdir -p "$out"

sh tests/smp/syscall-fuzz/raw-check.sh

if [ "${RAVEN_SYSCALL_FUZZ_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="$rustflags" \
    cargo build -p raven-syscall-fuzz --target "$target_triple" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$binary"
