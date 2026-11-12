#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S40 teardown fixture (`/init`, `/server`, `/s40-fault`, `/s40-namecheck`) and pack
# build/fixtures/s40-teardown/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s40-teardown
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s40-teardown/linker.ld" \
    cargo build -p raven-s40-teardown --target x86_64-unknown-none --release
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target/raven-s40-init" \
    /server="$target/raven-s40-server" \
    /s40-fault="$target/raven-s40-fault" \
    /s40-namecheck="$target/raven-s40-namecheck"
