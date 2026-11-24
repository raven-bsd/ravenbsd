#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the aarch64 einkd supervision fixture: the BSP einkd binary at /sbin/einkd and a tiny
# /init wrapper that starts it with --dry-run --once. The qtest boots this initfs directly; it
# does not log in to the x86 release image.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

arch=${RAVEN_QT_ARCH:-aarch64}
if [ "$arch" != aarch64 ]; then
    printf 'einkd-supervision: only the aarch64 BSP path is supported (got %s)\n' "$arch" >&2
    exit 2
fi

bsp=${RAVENBSD_RPI:-$root/../ravenbsd-rpi}
if [ ! -f "$bsp/Cargo.toml" ]; then
    printf 'einkd-supervision: BSP checkout not found at %s (set RAVENBSD_RPI)\n' "$bsp" >&2
    exit 1
fi

sh "$bsp/scripts/build.sh"
cargo build --manifest-path "$bsp/Cargo.toml" -p einkd --bin einkd-supervision \
    --release --target aarch64-unknown-none-softfloat

target=aarch64-unknown-none-softfloat
out=build/fixtures/einkd-supervision
mkdir -p "$out"
cp "$bsp/build/aarch64/einkd" "$out/einkd"
cp "$bsp/target/$target/release/einkd-supervision" "$out/init"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$out/init" \
    /sbin/einkd="$out/einkd"

printf 'einkd-supervision: staged %s/initfs.img\n' "$out"
