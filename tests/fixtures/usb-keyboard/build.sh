#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S67 QEMU xHCI keyboard initfs:
#   /init             raven-init (svcd)
#   /sbin/pcid        PCI enumerator
#   /sbin/devd        device manager (one-shot test mode)
#   /sbin/xhci        the guest xHCI driver
#   /bin/svcctl       service utility (kept for diagnostics)
# plus the service manifests, drivers.toml and svcd.conf under /etc.
#
# The QEMU case supplies `-device qemu-xhci -device usb-kbd`.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/usb-keyboard
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p raven-svcctl -p raven-pcid -p raven-devd -p xhci \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-init:0755" \
    "/bin/svcctl=$target_dir/$target/release/raven-svcctl:0755" \
    "/sbin/pcid=$target_dir/$target/release/raven-pcid:0755" \
    "/sbin/devd=$target_dir/$target/release/raven-devd:0755" \
    "/sbin/xhci=$target_dir/$target/release/xhci:0755" \
    "/etc/services.d/pcid.toml=$root/tests/fixtures/usb-keyboard/etc/pcid.toml:0644" \
    "/etc/services.d/devd.toml=$root/tests/fixtures/usb-keyboard/etc/devd.toml:0644" \
    "/etc/drivers.toml=$root/tests/fixtures/usb-keyboard/etc/drivers.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/usb-keyboard/etc/svcd.conf:0644"
