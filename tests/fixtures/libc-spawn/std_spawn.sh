#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P9c std `Command` probe and install it as the libc-test program. The C73 driver
# stages libc-child and the placeholder files first.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

sh tests/libc/target/build.sh spawn x86_64

crt=$root/build/x86_64
out=$root/build/libc-test
target_dir=$root/build/cargo/std-spawn
manifest=$root/tests/fixtures/libc-spawn/std_spawn/Cargo.toml

if [ ! -f "$crt/crt1.o" ] || [ ! -f "$crt/libc.a" ]; then
    printf 'std-spawn: missing crt/libc under %s\n' "$crt" >&2
    exit 1
fi

RUSTUP_TOOLCHAIN=ravenbsd \
RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C relocation-model=static -C code-model=small \
-C link-arg=-T$root/xtask/brush-x86_64.ld \
-C link-arg=$crt/crt1.o -C link-arg=$crt/crti.o \
-C link-arg=-L$crt -C link-arg=-lc -C link-arg=$crt/crtn.o" \
    cargo build --manifest-path "$manifest" \
        --target x86_64-unknown-ravenbsd --release \
        --target-dir "$target_dir"

artifact=$target_dir/x86_64-unknown-ravenbsd/release/std-spawn
if [ ! -f "$artifact" ]; then
    printf 'std-spawn: missing %s\n' "$artifact" >&2
    exit 1
fi
cp "$artifact" "$out/libc-test"
