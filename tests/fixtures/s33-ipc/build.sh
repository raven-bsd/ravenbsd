#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S33 IPC-lifecycle fixture (`/init`, `/server`, `/s33-loop`, `/s33-exit`,
# `/s33-block`) and pack build/fixtures/s33-ipc/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s33-ipc
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s33-ipc/linker.ld" \
    cargo build -p raven-s33-ipc --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s33-init \
    /server=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s33-server \
    /s33-loop=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s33-loop \
    /s33-exit=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s33-exit \
    /s33-block=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release/raven-s33-block
