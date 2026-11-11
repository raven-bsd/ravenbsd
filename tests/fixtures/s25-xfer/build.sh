#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S25 handle-transfer fixture (`/init`, `/child`, `/server`) and pack
# build/fixtures/s25-xfer/initfs.img. Flags follow tickets/stuart/reports/S25.md.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s25-xfer
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s25-xfer/linker.ld" \
    cargo build -p raven-s25-xfer --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=target/x86_64-unknown-none/release/raven-s25-init \
    /child=target/x86_64-unknown-none/release/raven-s25-child \
    /server=target/x86_64-unknown-none/release/raven-s25-server
