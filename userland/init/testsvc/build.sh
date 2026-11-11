#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the svcd qtest's test service with a page-aligned linker script and install it where the
# `svcd-test` profile expects a file. `raven_rt::spawn` is strict about unaligned segment tails,
# while the production user linker script leaves them unaligned.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/svcd-test
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/init/testsvc/linker.ld" \
    cargo build -p raven-testsvc --target x86_64-unknown-none

# Honour CARGO_TARGET_DIR: the gate/ci runs cargo with a shared target directory.
target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-testsvc" "$out/testsvc"
