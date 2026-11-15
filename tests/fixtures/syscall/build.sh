#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S55 syscall-negative fixture and pack build/fixtures/syscall/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/syscall
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "syscall-negative fixture: only x86_64 is supported" >&2
        exit 1
        ;;
esac
manifest=tests/fixtures/syscall/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-syscall-negative --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-syscall-negative"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/syscall/Cargo.lock
