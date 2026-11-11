#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the core-servers qtest client and install it where the `core-test` profile expects a
# generated file. The user linker page-aligns every segment end, so spawn(Strict) accepts it.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/core-test
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-coretest --target x86_64-unknown-none

# Honour CARGO_TARGET_DIR: the gate/ci runs cargo with a shared target directory.
target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-coretest" "$out/coretest"
