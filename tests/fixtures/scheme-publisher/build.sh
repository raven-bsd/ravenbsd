#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K532 scheme-publisher fixture and pack build/fixtures/scheme-publisher/initfs.img.
# The trusted publisher and the setuid child are installed 4755 in the boot-trusted initfs.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/scheme-publisher
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/target}
target=x86_64-unknown-none
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/tests/fixtures/scheme-publisher/Cargo.toml" \
    --target "$target" --release

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
    cargo build -p raven-initfsd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-scheme-publisher-init" \
    "/sbin/initfsd=$target_dir/$target/release/raven-initfsd" \
    "/sbin/fake=$target_dir/$target/release/raven-scheme-publisher-fake" \
    "/sbin/trusted=$target_dir/$target/release/raven-scheme-publisher-trusted:4755" \
    "/sbin/probe=$target_dir/$target/release/raven-scheme-publisher-probe" \
    "/bin/child=$target_dir/$target/release/raven-scheme-publisher-child:0755" \
    "/bin/setuid-child=$target_dir/$target/release/raven-scheme-publisher-child:4755"
