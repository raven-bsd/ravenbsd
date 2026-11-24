#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the U3b setuid/setgid fixture and pack build/fixtures/setuid/initfs.img.
# The same child ELF is installed at several paths with the modes the matrix needs.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/setuid
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-setuid -p raven-initfsd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-setuid-init" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/bin/child=$target_dir/$target/release/raven-setuid-child:0755" \
    "/bin/child-0644=$target_dir/$target/release/raven-setuid-child:0644" \
    "/bin/child-4644=$target_dir/$target/release/raven-setuid-child:4644" \
    "/bin/child-4755=$target_dir/$target/release/raven-setuid-child:4755" \
    "/bin/child-2755=$target_dir/$target/release/raven-setuid-child:2755" \
    "/bin/child-script=$root/tests/fixtures/setuid/script:4755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/auxv-initfs.img" \
    "/init=$target_dir/$target/release/raven-setuid-auxv" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/bin/child=$target_dir/$target/release/raven-setuid-child:0755"
