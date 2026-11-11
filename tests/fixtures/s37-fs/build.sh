#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S37 filesystem-plumbing fixture and pack build/fixtures/s37-fs/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s37-fs
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s37-fs/linker.ld" \
    cargo build -p raven-s37-fs --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s37-init \
    /server=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s37-server \
    /client=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s37-client \
    /death-client=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s37-death-client \
    /death-reader=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s37-death-reader
