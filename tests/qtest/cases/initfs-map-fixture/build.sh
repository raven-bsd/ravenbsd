#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K140 initfs-map fixture and pack build/fixtures/initfs-map/initfs.img.
# The fixture is a standalone package (its own [workspace]) so it does not add a product
# workspace member outside the ticket's Owns list.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/initfs-map
mkdir -p "$out"
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
        echo "initfs-map fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

manifest=tests/qtest/cases/initfs-map-fixture/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-initfs-map-fixture --target "$target" --release

# A 4 MiB deterministic blob gives the 3 MiB/768-page first-touch probe a valid source range.
dd if=/dev/zero of="$out/blob" bs=1048576 count=4 status=none

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-initfs-map-fixture" \
    /blob="$out/blob"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/qtest/cases/initfs-map-fixture/Cargo.lock
