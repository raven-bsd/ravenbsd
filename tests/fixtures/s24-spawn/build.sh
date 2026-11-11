#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S24 spawn fixture (`/init` parent and `/child`) and pack
# build/fixtures/s24-spawn/initfs.img. Flags follow tickets/stuart/reports/S24.md.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s24-spawn
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s24-spawn/linker.ld" \
    cargo build -p raven-s24-parent -p raven-s24-child --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s24-parent \
    /child=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s24-child
