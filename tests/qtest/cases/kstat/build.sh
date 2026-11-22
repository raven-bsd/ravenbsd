#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# KSTAT1 acceptance build: the typed kstat model, the userland tool and a private initfs that
# runs the tool as `/init`. The kernel is the normal `base` profile build.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

missing=""
for path in \
    kernel/src/kstat \
    crates/raven-kstat \
    userland/bin/kstat
do
    if [ ! -e "$path" ]; then
        missing="$missing $path"
    fi
done
if [ -n "$missing" ]; then
    printf 'kstat: KSTAT1 product missing:%s\n' "$missing" >&2
    exit 1
fi

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
        echo "kstat: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/kstat
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -q -p raven-kstat-cli --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-kstat-cli:0755"
