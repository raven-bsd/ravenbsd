#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K496 duplex semantics matrix fixture and pack build/duplex-matrix/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: duplex-matrix is x86_64 only\n'
    exit 77
fi

out=build/duplex-matrix
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
binary=$target/raven-duplex-matrix-init
mkdir -p "$out"

if [ "${RAVEN_DUPLEX_MATRIX_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/smp/duplex-matrix/linker.ld" \
    cargo build -p raven-duplex-matrix --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$binary"
