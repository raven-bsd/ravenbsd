#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S62 usb-keyboard fixture and pack build/fixtures/usb-keyboard/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/usb-keyboard
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "usb-keyboard fixture: only x86_64 is supported" >&2
        exit 1
        ;;
esac
manifest=tests/fixtures/usb-keyboard/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-usb-keyboard --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-usb-keyboard"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/usb-keyboard/Cargo.lock
