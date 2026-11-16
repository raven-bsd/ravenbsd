#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the D2 Pi 4 xHCI fake-keyboard initfs:
#   /init                 raven-init (svcd)
#   /sbin/pi4-xhci        the Pi 4 xHCI driver in test mode
# plus the service manifest and svcd.conf. The driver uses the D1 model and the S61/S62
# enumeration path; no PCI/MMIO hardware is touched by this qtest.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-usb
mkdir -p "$out"

target=aarch64-unknown-none-softfloat
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p pi4-xhci --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-init:0755" \
    "/sbin/pi4-xhci=$target_dir/$target/release/pi4-xhci:0755" \
    "/etc/services.d/pi4-xhci.toml=$root/tests/fixtures/pi4-usb/etc/pi4-xhci.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/pi4-usb/etc/svcd.conf:0644"
