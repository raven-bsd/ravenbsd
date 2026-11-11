#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S35 handle fixture and pack build/fixtures/s35-handles/initfs.img.
# The source-level panic-handler count is a preflight for the compiled kernel: the build fails on
# a second live handler anyway, and the count catches a dead duplicate before it confuses review.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

handlers=$(grep -rl --include='*.rs' '#\[panic_handler\]' kernel/src | wc -l)
if [ "$handlers" -ne 1 ]; then
    echo "s35: expected exactly one panic handler, found $handlers" >&2
    exit 1
fi

out=build/fixtures/s35-handles
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s35-handles/linker.ld" \
    cargo build -p raven-s35-handles --target x86_64-unknown-none --release

target_dir=${CARGO_TARGET_DIR:-$root/target}
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/x86_64-unknown-none/release/raven-s35-init"
