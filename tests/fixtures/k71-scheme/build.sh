#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K71 reply-attachment fixture (`/init`, `/server`) and pack
# build/fixtures/k71-scheme/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k71-scheme
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/k71-scheme/linker.ld" \
    cargo build -p raven-k71-scheme --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/x86_64-unknown-none/release/raven-k71-init" \
    /server="$target_dir/x86_64-unknown-none/release/raven-k71-server"
