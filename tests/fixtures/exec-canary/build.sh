#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the Rust std Command canary as a static x86_64-unknown-ravenbsd ET_EXEC, using the same
# target/CRT/libc vector as `cargo xtask build brush` (xtask/src/brush.rs) and the brush linker
# script. The output is staged by profiles/libc-test.d/10-libc-test.toml at /bin/exec-canary.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
target=x86_64-unknown-ravenbsd
crt=$root/build/$arch
out=$root/build/fixtures/exec-canary

RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small -C link-arg=-T$root/xtask/brush-x86_64.ld -C link-arg=$crt/crt1.o -C link-arg=$crt/crti.o -C link-arg=-L$crt -C link-arg=-lc -C link-arg=$crt/crtn.o"
export RUSTFLAGS

cargo +ravenbsd build --manifest-path "$root/tests/fixtures/exec-canary/Cargo.toml" \
    --bin exec-canary --target "$target" --release \
    --target-dir "$out/target"

mkdir -p "$out"
cp "$out/target/$target/release/exec-canary" "$out/exec-canary"
