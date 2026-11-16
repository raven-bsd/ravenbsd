#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P2j pipe fixtures. Each qtest gets its own initfs:
#   pipe.img       -> /init = pipe-init
#   pipe-fork.img  -> /init = pipe-fork-init

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
        echo "pipe fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/pipe
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p raven-pipe-tests --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/pipe.img" \
    "/init=$target_dir/$target/release/pipe-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/pipe-fork.img" \
    "/init=$target_dir/$target/release/pipe-fork-init:0755"
