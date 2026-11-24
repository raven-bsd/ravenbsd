#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K466 durability probe and a fresh external UFS2 disk with the probe
# injected at /bin/ufs-probe. Usage: build.sh <4m|16m>
#
# The disk is a one-partition GPT around a UFS2 payload, the same shape as the
# S99 demo disk, so the ufs-demo profile's fsd-ufs binds it and the shell can
# mount it at /mnt.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

size=${1:-16m}
out=build/fixtures/ufs-durability
mkdir -p "$out"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        linker="$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        linker="$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "ufs-durability fixture: unsupported qtest arch \`$arch\`" >&2
        exit 2
        ;;
esac

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker" \
    cargo build -p raven-ufs-durability-probe --target "$target" --release

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/$target/release/raven-ufs-durability-probe" "$out/ufs-probe"

rm -f "$out/ufs.img" "$out/disk.img"
cargo run -q -p ufs-mkfs -- -O 2 -s "$size" -T 0 "$out/ufs.img"
cargo run -q -p raven-k54-mkimage -- "$out/ufs.img" "$out/disk.img"
scripts/raven-ufs put "$out/ufs-probe" /bin/ufs-probe "$out/disk.img"
scripts/raven-ufs ls "$out/disk.img" /bin
