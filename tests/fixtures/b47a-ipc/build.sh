#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B47a aarch64 IPC fixture and pack build/fixtures/b47a-ipc/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b47a-ipc
mkdir -p "$out"
target=aarch64-unknown-none-softfloat
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build -p raven-b47a-ipc -p raven-init -p raven-testsvc --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-b47a-init" \
    /client="$target_dir/$target/release/raven-b47a-client" \
    /server="$target_dir/$target/release/raven-b47a-server" \
    /svcd="$target_dir/$target/release/raven-init" \
    /sbin/b47a-svc="$target_dir/$target/release/raven-testsvc" \
    /etc/services.d/b47a.toml="$root/tests/fixtures/b47a-ipc/etc/b47a.toml"
