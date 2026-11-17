#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the one-shot tooltest and the ten first-party file tools for the qtest architecture.
# The profile stages them as static generated files so tooltest's initfs-only spawn path can
# run them on both architectures.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
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
        echo "file-tools fixture: unsupported architecture: $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/file-tools
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/target}
tools="mkdir touch cp cat ls stat df mv rm rmdir"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-mkdir -p raven-touch -p raven-cp -p raven-cat -p raven-ls \
        -p raven-stat -p raven-df -p raven-mv -p raven-rm -p raven-rmdir \
        --target "$target" --target-dir "$target_dir"
RUSTFLAGS="$rustflags" \
    cargo build -p raven-tool-core --bin raven-tool-core --target "$target" \
        --target-dir "$target_dir"

for name in $tools; do
    cp "$target_dir/$target/debug/raven-$name" "$out/$name"
done
cp "$target_dir/$target/debug/raven-tool-core" "$out/tooltest"
