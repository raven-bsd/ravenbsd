#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S731 create-semantics fixture and pack
# build/conformance/credentials/create-umask/initfs.img. `/init` is the fixture; the product
# `initfsd`, `ramfsd` and `vfsd` binaries serve the real `file:` namespace.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/conformance/credentials/create-umask
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
        echo "create-umask fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac
target_dir=${CARGO_TARGET_DIR:-$root/target}
binary="$target_dir/$target/release/raven-create-conformance"

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$root/tests/conformance/credentials/create-umask/Cargo.toml" \
    --target "$target" --release --target-dir "$target_dir"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    --target "$target" --release --target-dir "$target_dir"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$binary:0755" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd:0755" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd:0755" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd:0755"
