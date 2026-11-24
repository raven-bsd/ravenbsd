#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K147 scheme call-cost fixture (`/init`, `/server`, `/client`) and pack
# build/fixtures/scheme-call-cost/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/scheme-call-cost
target=build/cargo/scheme-call-cost
manifest=tests/qtest/cases/scheme-call-cost/Cargo.toml
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/qtest/cases/scheme-call-cost/linker.ld" \
    cargo build --manifest-path "$manifest" --target x86_64-unknown-none --release --target-dir "$target"
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target/x86_64-unknown-none/release/raven-scheme-call-cost-init" \
    /server="$target/x86_64-unknown-none/release/raven-scheme-call-cost-server" \
    /client="$target/x86_64-unknown-none/release/raven-scheme-call-cost-client"
