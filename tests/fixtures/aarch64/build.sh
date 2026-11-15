#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S56 aarch64 libc-parity fixture and pack build/fixtures/aarch64/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/aarch64
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-aarch64}
case "$arch" in
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "aarch64-libc fixture: only aarch64 is supported" >&2
        exit 1
        ;;
esac
manifest=tests/fixtures/aarch64/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-aarch64-libc --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-aarch64-libc"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/aarch64/Cargo.lock
