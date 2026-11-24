#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S118 x86_64 concurrent TLB-shootdown regression and pack
# build/smp-x86/initfs.img. The fixture is x86_64 only; another arch exits 77 so the qtest
# records a skip instead of a false pass.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp-x86 concurrent regression is x86_64 only\n'
    exit 77
fi

out=build/smp-x86
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
binary=$target/raven-smp-x86-concurrent-init
mkdir -p "$out"

if [ "${RAVEN_SMP_X86_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-smp-x86-concurrent --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$binary"
