#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the Rust std Command canary as a static x86_64-unknown-ravenbsd ET_EXEC. The binary is the
# `exec-canary` member of the brush-shell crate (the same std/tokio dependency vector as brush) so
# it runs the Command inside the current-thread Tokio runtime the shell uses on RavenBSD.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
target=x86_64-unknown-ravenbsd
crt=$root/build/$arch
out=$root/build/fixtures/exec-canary

RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small -C link-arg=-T$root/xtask/brush-x86_64.ld -C link-arg=$crt/crt1.o -C link-arg=$crt/crti.o -C link-arg=-L$crt -C link-arg=-lc -C link-arg=$crt/crtn.o"
export RUSTFLAGS

RUSTUP_TOOLCHAIN=ravenbsd cargo build --manifest-path "$root/userland/brush/Cargo.toml" \
    -p brush-shell --bin exec-canary --target "$target" --release \
    --target-dir "$out/target"

mkdir -p "$out"
cp "$out/target/$target/release/exec-canary" "$out/exec-canary"
