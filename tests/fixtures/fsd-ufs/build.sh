#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the fsd-ufs probe for the architecture selected by the qtest runner and
# wrap the B63 UFS2 golden image in the one-partition GPT disk the K14 RAM block
# device serves.
#
# The `fsd-ufs-test` profile stages the probe and disk from build/fixtures/k54-ufs.
# This script keeps that generated path stable while selecting the probe target
# from RAVEN_QT_ARCH, so `cargo xtask qtest --case fsd-ufs --arch <arch>` builds
# a matching probe.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

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
        echo "fsd-ufs fixture: unsupported qtest arch \`$arch\`" >&2
        exit 2
        ;;
esac

out=build/fixtures/k54-ufs
mkdir -p "$out"

gzip -dc tests/ufs-vectors/golden/ufs2-b4096.img.gz > "$out/ufs.img"
cargo run -q -p raven-k54-mkimage -- "$out/ufs.img" "$out/disk.img"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker" \
    cargo build -p raven-k54-probe --target "$target"

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/$target/debug/raven-k54-probe" "$out/probe"
