#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B71a spawn-smoke /init and pack build/fixtures/spawn/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/spawn
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "spawn fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac
target_dir="$out/target"

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/tests/fixtures/spawn/Cargo.toml" \
    --bins --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-spawn-smoke" \
    "/child=$target_dir/$target/release/child"
