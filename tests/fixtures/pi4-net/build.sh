#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the pi4-net-model guest and pack its initfs with the good and duplicate
# chain fixtures. This is a host model, not a board image.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-net
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-$root/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-pi4-net-model --target "$target" --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-pi4-net-model" \
    /etc/pi4-net-good.chain="tests/fixtures/pi4-net/model-good.chain" \
    /etc/pi4-net-duplicate.chain="tests/fixtures/pi4-net/model-duplicate.chain"
