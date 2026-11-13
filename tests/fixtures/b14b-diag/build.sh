#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B14b net-diag fixture and pack build/fixtures/b14b-diag/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b14b-diag
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-b14b-diag -p raven-ping -p raven-route -p raven-netstat -p netd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-b14b-diag-init" \
    /bin/ping="$target_dir/$target/release/raven-ping" \
    /bin/route="$target_dir/$target/release/raven-route" \
    /bin/netstat="$target_dir/$target/release/raven-netstat" \
    /sbin/netd="$target_dir/$target/release/netd"
