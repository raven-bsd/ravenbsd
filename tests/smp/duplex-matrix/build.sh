#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K496 duplex semantics matrix fixture and pack build/duplex-matrix/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target_triple=x86_64-unknown-none
        linker=$root/tests/smp/duplex-matrix/linker.ld
        out=build/duplex-matrix
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker"
        ;;
    aarch64)
        target_triple=aarch64-unknown-none-softfloat
        linker=$root/tests/smp/duplex-matrix/linker-aarch64.ld
        out=build/duplex-matrix/aarch64
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$linker"
        ;;
    *)
        printf 'skipped: duplex-matrix supports x86_64 and aarch64 (got %s)\n' "$arch"
        exit 77
        ;;
esac

target=${CARGO_TARGET_DIR:-target}/$target_triple/release
binary=$target/raven-duplex-matrix-init
mkdir -p "$out"

if [ "${RAVEN_DUPLEX_MATRIX_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="$rustflags" \
    cargo build -p raven-duplex-matrix --target "$target_triple" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$binary"
