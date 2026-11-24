#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S188: build the multi-threaded Tokio shell-shape discriminator into build/libc-test/libc-test.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
[ "$arch" = x86_64 ] || { printf 'tls-tokio: only x86_64\n' >&2; exit 2; }

out=$root/build/libc-test
mkdir -p "$out"
target_dir=$root/build/libc/tls/tokio-target
crt=$root/build/x86_64

RUSTUP_TOOLCHAIN=ravenbsd CARGO_TARGET_DIR="$target_dir" \
RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small \
-C link-arg=-T$root/tests/libc/tls/x86_64.ld \
-C link-arg=$crt/crt1.o -C link-arg=$crt/crti.o -C link-arg=-L$crt -C link-arg=-lc -C link-arg=$crt/crtn.o" \
    cargo build --manifest-path "$root/tests/libc/tls/tokio-exit/Cargo.toml" \
        --target x86_64-unknown-ravenbsd --release

cp "$target_dir/x86_64-unknown-ravenbsd/release/s188-tokio-exit" "$out/libc-test"
: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"
printf 'tls-tokio-exit\n' > "$out/stamp"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
printf 'tls-tokio-exit: staged %s\n' "$out/libc-test"
