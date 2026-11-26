#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S204 vfs-umask probe for the architecture selected by the qtest
# runner and install it where the `vfsd-test` profile expects its generated
# one-shot probe. The profile stages `/sbin/vfsd-probe` from
# `build/fixtures/vfsd-probe/probe`, so this fixture writes that path.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/vfs-umask
mkdir -p "$out"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        linker="$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "vfs-umask fixture: unsupported qtest arch \`$arch\`" >&2
        exit 2
        ;;
esac

target_dir=${CARGO_TARGET_DIR:-$root/target}
manifest=tests/fixtures/vfs-umask/Cargo.toml

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-vfs-umask --target "$target" --release

cp "$target_dir/$target/release/raven-vfs-umask" "$out/probe"

# The reused `vfsd-test` profile stages its one-shot service from this path.
mkdir -p build/fixtures/vfsd-probe
cp "$out/probe" build/fixtures/vfsd-probe/probe
