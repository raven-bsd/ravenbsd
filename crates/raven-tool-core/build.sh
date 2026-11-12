#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the one-shot file-tools integration client and place it where the
# `file-tools-test` profile expects a generated initfs file.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

out=build/fixtures/file-tools
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-tool-core --bin raven-tool-core --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cp "$target_dir/x86_64-unknown-none/debug/raven-tool-core" "$out/tooltest"
