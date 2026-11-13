#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B70 probe and pack the test-only initfs overlay. The profile extends
# the production `server` image; this script only adds the probe and replaces
# `/etc/svcd.conf` so the probe's exit status becomes the QEMU status.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b70-net
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-$root/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-b70-probe --release --target "$target"

cargo build -p raven-b70-overlay

cp "$target_dir/$target/release/raven-b70-probe" "$out/probe"

RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile hw-net > /dev/null
cp build/x86_64/initfs.img "$out/base.img"

"$target_dir/debug/raven-b70-overlay" \
    "$out/base.img" \
    "$out/initfs.img" \
    "/etc/svcd.conf=profiles/hw-net.d/svcd.conf"
