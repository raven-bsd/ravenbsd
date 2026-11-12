#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S38 event close-hook fixture and pack build/fixtures/s38-events/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s38-events
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s38-events/linker.ld" \
    cargo build -p raven-s38-events --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=target/x86_64-unknown-none/release/raven-s38-events
