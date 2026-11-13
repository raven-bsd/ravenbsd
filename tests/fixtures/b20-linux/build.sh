#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B20 fixture and pack build/fixtures/b20-linux/initfs.img:
#   /init  = the linuxd server (Raven target)
#   /hello = the static musl binary the server loads
#
# A missing `musl-gcc` fails the qtest instead of skipping it.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b20-linux
mkdir -p "$out"

if ! command -v musl-gcc >/dev/null 2>&1; then
    echo "b20-linux: musl-gcc is not installed (install musl-tools; the case is not skipped)" >&2
    exit 1
fi

musl-gcc -static -O0 -o "$out/hello" tests/fixtures/b20-linux/hello.c

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p linuxd --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/linuxd" \
    /hello="$out/hello"
