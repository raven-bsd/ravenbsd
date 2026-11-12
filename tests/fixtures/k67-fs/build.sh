#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K67 conformance fixture and the three real filesystem servers,
# then pack them into one initfs.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k67-fs
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/k67-fs/linker.ld" \
    cargo build \
    -p raven-k67-fs \
    -p raven-initfsd \
    -p raven-ramfsd \
    -p raven-vfsd \
    --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/x86_64-unknown-none/release/raven-k67-fs" \
    /client="$target_dir/x86_64-unknown-none/release/raven-k67-client" \
    /sbin/initfsd="$target_dir/x86_64-unknown-none/release/raven-initfsd" \
    /sbin/ramfsd="$target_dir/x86_64-unknown-none/release/raven-ramfsd" \
    /sbin/vfsd="$target_dir/x86_64-unknown-none/release/raven-vfsd" \
    /etc/mounts="$root/tests/fixtures/k67-fs/etc/mounts"
