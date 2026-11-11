#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S26 event-queue fixture and pack build/fixtures/s26-evq/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s26-evq
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s26-evq/linker.ld" \
    cargo build -p raven-s26-evq --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s26-init \
    /server=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s26-server \
    /client=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s26-client \
    /signaler=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s26-signaler
