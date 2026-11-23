#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 rev 106: bounded S36 thread churn row. Builds the existing process-lifecycle
# fixture with the churn-only feature and packs it as an initfs; no product code.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s36-churn
mkdir -p "$out"
CARGO_TARGET_DIR="$out/target" \
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s36-lifecycle/linker.ld" \
    cargo build -p raven-s36-lifecycle --target x86_64-unknown-none --release \
    --features s36-churn-only
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$out/target/x86_64-unknown-none/release/raven-s36-lifecycle"
