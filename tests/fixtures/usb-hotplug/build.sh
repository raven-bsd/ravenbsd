#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S68 HID hotplug fixture and pack build/fixtures/usb-hotplug/initfs.img.
# The fixture runs a fake USB core that can go absent and return, so no QEMU USB device is needed.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/usb-hotplug
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
manifest=tests/fixtures/usb-hotplug/Cargo.toml
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/usb-hotplug/linker.ld"

CARGO_TARGET_DIR="$target_dir" RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-usb-hotplug"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/usb-hotplug/Cargo.lock
