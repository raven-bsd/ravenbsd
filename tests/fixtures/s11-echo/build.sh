#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S11 scheme-echo fixture (`/init`, `/server`, `/client`) and pack
# build/fixtures/s11-echo/initfs.img. Flags follow tickets/stuart/reports/S11.md.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s11-echo
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s11-echo/linker.ld" \
    cargo build -p raven-s11-echo --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s11-init \
    /server=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s11-server \
    /client=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s11-client
