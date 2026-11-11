#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S32 capture-lifetime fixture (`/init`, `/server`, `/client`, `/canceller`) and pack
# build/fixtures/s32-capture/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s32-capture
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s32-capture/linker.ld" \
    cargo build -p raven-s32-capture --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init=target/x86_64-unknown-none/release/raven-s32-init \
    /server=target/x86_64-unknown-none/release/raven-s32-server \
    /client=target/x86_64-unknown-none/release/raven-s32-client \
    /canceller=target/x86_64-unknown-none/release/raven-s32-canceller
