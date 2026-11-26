#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K571: build the no-brush handler/raise/ucontext/return fixture into build/libc-test/libc-test.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
[ "$arch" = x86_64 ] || { printf 'k571-signal: only x86_64\n' >&2; exit 77; }

sysroot=$root/build/$arch/sysroot
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" k571-signal

out=$root/build/libc-test
mkdir -p "$out"
target_dir=$root/build/libc/signals/k571-target
# The fixture links the packed libc.a through explicit link args, which cargo does not track;
# drop the fixture target so a repacked libc is really relinked.
rm -rf "$target_dir"

RUSTUP_TOOLCHAIN=ravenbsd CARGO_TARGET_DIR="$target_dir" \
RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small \
-C link-arg=-T$root/tests/libc/tls/x86_64.ld \
-C link-arg=$sysroot/usr/lib/crt1.o -C link-arg=$sysroot/usr/lib/crti.o \
-C link-arg=-L$sysroot/usr/lib -C link-arg=-lc -C link-arg=$sysroot/usr/lib/crtn.o" \
    cargo build --manifest-path "$root/tests/libc/signals/k571-signal/Cargo.toml" \
        --target x86_64-unknown-ravenbsd --release

cp "$target_dir/x86_64-unknown-ravenbsd/release/k571-signal" "$out/libc-test"
: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"
printf 'k571-signal\n' > "$out/stamp"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
printf 'k571-signal: staged %s\n' "$out/libc-test"
