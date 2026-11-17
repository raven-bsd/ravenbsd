#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S75 x86-64 user SMP fixture and pack build/fixtures/smp-user/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp-user is x86_64 only\n'
    exit 77
fi

out=build/fixtures/smp-user
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/smp-user/linker.ld" \
    cargo build -p raven-smp-user --target x86_64-unknown-none --release
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target/raven-smp-user-init"
