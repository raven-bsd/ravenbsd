#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K529 capability fixture (root controller + uid-1000 child) and pack
# build/fixtures/capability/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "capability fixture: only x86_64 is supported" >&2
        exit 1
        ;;
esac

out=build/fixtures/capability
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}
manifest=tests/qtest/cases/capability.d/Cargo.toml

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-capability --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-capability-init:0755" \
    "/child=$target_dir/$target/release/raven-capability-child:0755"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/qtest/cases/capability.d/Cargo.lock
