#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S72 time-entropy fixture and pack build/fixtures/time-entropy/initfs.img.
# The architecture is chosen by the harness (`RAVEN_QT_ARCH`), defaulting to x86_64.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/time-entropy
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
        echo "time-entropy fixture: unsupported arch $arch" >&2
        exit 1
        ;;
esac
manifest=tests/fixtures/time-entropy/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-time-entropy --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-time-entropy"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/time-entropy/Cargo.lock
