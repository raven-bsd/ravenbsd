#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S39 aarch64 parity fixture and pack build/fixtures/s39-parity/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s39-parity
mkdir -p "$out"
target=aarch64-unknown-none-softfloat
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-s39-parity -p raven-init -p raven-testsvc --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-s39-init" \
    /client="$target_dir/$target/release/raven-s39-client" \
    /server="$target_dir/$target/release/raven-s39-server" \
    /svcd="$target_dir/$target/release/raven-init" \
    /sbin/s39-svc="$target_dir/$target/release/raven-testsvc" \
    /etc/services.d/s39.toml="$root/tests/fixtures/s39-parity/etc/s39.toml"
