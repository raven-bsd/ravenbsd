#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K469 IPC message fuzz fixture and pack build/ipc-fuzz/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: ipc-fuzz is x86_64 only\n'
    exit 77
fi

out=build/ipc-fuzz
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
binary=$target/raven-ipc-fuzz-init
mkdir -p "$out"

if [ "${RAVEN_IPC_FUZZ_NO_BUILD:-0}" = 1 ] && [ -f "$out/initfs.img" ] && [ -f "$binary" ]; then
    exit 0
fi

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/smp/ipc-fuzz/linker.ld" \
    cargo build -p raven-ipc-fuzz --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$binary"
