#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K484 aarch64 memory-parity init (PROT_NONE + frame exhaustion) and pack
# build/smp-aarch64-memory/initfs.img. Another arch exits 77 so the qtest records a skip.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-aarch64}}
if [ "$arch" != aarch64 ]; then
    printf 'skipped: smp-aarch64 memory parity is aarch64 only\n'
    exit 77
fi

out=build/smp-aarch64-memory
target=${CARGO_TARGET_DIR:-target}/aarch64-unknown-none-softfloat/release
binary=$target/raven-smp-aarch64-memory-init
mkdir -p "$out"

if [ "${RAVEN_SMP_AARCH64_MEMORY_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-smp-aarch64 --bin raven-smp-aarch64-memory-init \
        --target aarch64-unknown-none-softfloat --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$binary:0755"
