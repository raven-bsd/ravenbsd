#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S69 process-death cleanup fixture and pack
# build/fixtures/proc-death-cleanup/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case $arch in
    x86_64)
        target=x86_64-unknown-none
        linker=$root/tests/fixtures/proc-death-cleanup/linker.ld
        extra="-C code-model=small -C lto=no"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        linker=$root/userland/linker/aarch64.ld
        extra="-C force-frame-pointers=yes"
        ;;
    *)
        printf 'unsupported arch %s\n' "$arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/proc-death-cleanup
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="-C relocation-model=static $extra -C link-arg=-T$linker" \
    cargo build -p raven-proc-death-cleanup --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-s69-init"
