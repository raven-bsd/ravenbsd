#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S16 rights/namespace fixture and pack build/fixtures/s16-rights/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s16-rights
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s16-rights/linker.ld" \
    cargo build -p raven-s16-rights --target x86_64-unknown-none --release
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target/raven-s16-init" \
    /server="$target/raven-s16-server" \
    /revoke="$target/raven-s16-revoke" \
    /prefix="$target/raven-s16-prefix" \
    /clear="$target/raven-s16-clear" \
    /exact="$target/raven-s16-exact"
