#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P4y crond qtest initfs: core VFS services, crond, echo and the fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/crontab
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-crontab-fixture -p raven-initfsd -p raven-ramfsd -p raven-vfsd \
    -p crond -p raven-echo --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-crontab-fixture" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd" \
    "/sbin/vfsd=$target_dir/$target/release/raven-vfsd" \
    "/sbin/crond=$target_dir/$target/release/crond" \
    "/bin/echo=$target_dir/$target/release/raven-echo"
