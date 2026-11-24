#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K528 capability-lineage race fixture and pack build/smp/handles/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp handles is x86_64 only\n'
    exit 77
fi

out=build/smp/handles
mkdir -p "$out"
CARGO_TARGET_DIR="$out/target" \
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/smp/handles/linker.ld" \
    cargo build --manifest-path tests/smp/handles/Cargo.toml --target x86_64-unknown-none --release

target="$out/target/x86_64-unknown-none/release/smp-handles-init"
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" /init="$target"
