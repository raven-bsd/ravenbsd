#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S17 pager fixture (`/init`, `/server`, `/fault-client`) and pack
# build/fixtures/s17-pager/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s17-pager
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s17-pager/linker.ld" \
    cargo build -p raven-s17-pager --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s17-init \
    /server=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s17-server \
    /fault-client=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s17-fault-client
