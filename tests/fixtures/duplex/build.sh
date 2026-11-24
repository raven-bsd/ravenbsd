#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K208 duplex endpoint qtest fixture as a standalone workspace.
# The image is /init = duplex-init.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "duplex fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/duplex
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/target}

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/tests/fixtures/duplex/Cargo.toml" \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/duplex.img" \
    "/init=$target_dir/$target/release/duplex-init:0755"
