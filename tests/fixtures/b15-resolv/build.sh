#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the loopback DNS fixture and the resolver probe.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b15-resolv
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-b15-dnsd --target x86_64-unknown-none

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-b15-probe --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-b15-dnsd" "$out/dnsd"
cp "$target_dir/x86_64-unknown-none/debug/raven-b15-probe" "$out/probe"
