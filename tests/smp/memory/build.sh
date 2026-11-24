#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S85b SMP memory stress fixture and pack build/smp-memory/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp-memory is x86_64 only\n'
    exit 77
fi

out=build/smp-memory
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
binary=$target/raven-smp-memory-init
mkdir -p "$out"

if [ "${RAVEN_SMP_MEMORY_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/smp/memory/linker.ld" \
    cargo build -p raven-smp-memory --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$binary"
