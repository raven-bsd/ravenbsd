#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S76c aarch64 soak fixtures and pack two initfs images:
#   smp-soak-aarch64.img      -> /init = smp-soak-aarch64-init      (smp_user=on)
#   smp-soak-off-aarch64.img  -> /init = smp-soak-off-aarch64-init  (smp_user=off)

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-aarch64}
case "$arch" in
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "smp-soak-aarch64 fixture: aarch64 only, got $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/smp-soak-aarch64
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p raven-smp-soak-aarch64 --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/smp-soak-aarch64.img" \
    "/init=$target_dir/$target/release/smp-soak-aarch64-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/smp-soak-off-aarch64.img" \
    "/init=$target_dir/$target/release/smp-soak-off-aarch64-init:0755"
