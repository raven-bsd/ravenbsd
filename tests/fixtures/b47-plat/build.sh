#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B47b platd fixture and pack build/fixtures/b47-plat/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b47-plat
mkdir -p "$out"
target=aarch64-unknown-none-softfloat
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-b47-plat -p raven-platd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-b47-plat-init" \
    /client="$target_dir/$target/release/raven-b47-plat-client" \
    /platd="$target_dir/$target/release/raven-platd"
