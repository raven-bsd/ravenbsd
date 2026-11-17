#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S73 capability handoff/revocation fixture and pack build/fixtures/cap-revoke/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

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
        echo "cap-revoke fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/cap-revoke
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p raven-cap-revoke --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-cap-revoke-init:0755" \
    "/client1=$target_dir/$target/release/raven-cap-revoke-client1:0755" \
    "/client2=$target_dir/$target/release/raven-cap-revoke-client2:0755"
