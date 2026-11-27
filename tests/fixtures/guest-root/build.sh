#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S777 guest-root fixture, the real initfsd server, and pack
# build/fixtures/guest-root/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/guest-root
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "guest-root fixture: x86_64 only" >&2
    exit 1
fi
target=x86_64-unknown-none
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
target_dir=${CARGO_TARGET_DIR:-target}
manifest=tests/fixtures/guest-root/Cargo.toml

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-guest-root-fixture --target "$target" --release

RUSTFLAGS="$rustflags" \
    cargo build -p raven-initfsd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-guest-root-fixture" \
    /initfsd="$target_dir/$target/release/raven-initfsd" \
    /root/.profile=tests/fixtures/guest-root/profile:0644

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/guest-root/Cargo.lock
