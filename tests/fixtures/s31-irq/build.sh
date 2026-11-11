#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S31 IRQ lifecycle fixture and pack build/fixtures/s31-irq/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s31-irq
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s31-irq/linker.ld" \
    cargo build -p raven-s31-irq --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=target/x86_64-unknown-none/release/raven-s31-irq
