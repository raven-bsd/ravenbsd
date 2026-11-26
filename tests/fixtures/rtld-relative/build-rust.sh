#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C74o regression: build a normally linked Rust PIE with PT_INTERP and DT_NEEDED libc.so.1, then
# embed it in the static dynamic-exec launcher and stage it as /bin/libc-test. The PIE does not
# export a C main; before the start-ABI fix the loader rejected libc's strong undefined main.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: rtld-relative-rust is x86_64 only\n'
    exit 77
fi

out=build/fixtures/rtld-relative
mkdir -p "$out"

sysroot=${RAVENLIBC_SYSROOT:-$root/build/x86_64/sysroot}
if [ ! -f "$sysroot/libexec/ld-elf.so.1" ] ||
    [ ! -f "$sysroot/lib/libc.so.1" ] ||
    [ ! -f "$sysroot/usr/lib/libc.so" ]; then
    cargo xtask libc-pack --arch x86_64
    cargo xtask libc-link --arch x86_64
fi

readelf=${READELF:-}
if [ -z "$readelf" ]; then
    if command -v llvm-readelf >/dev/null 2>&1; then
        readelf=llvm-readelf
    else
        readelf=readelf
    fi
fi

target_dir=${CARGO_TARGET_DIR:-$root/target}
RUSTFLAGS="-C relocation-model=pic -C link-arg=-pie \
    -C link-arg=--dynamic-linker=/libexec/ld-elf.so.1 \
    -C link-arg=-L$sysroot/usr/lib -C link-arg=-L$sysroot/lib -C link-arg=-lc" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/tests/fixtures/rtld-relative/rust-pie/Cargo.toml" \
    -p rtld-relative-rust --target x86_64-unknown-none --release
pie=$target_dir/x86_64-unknown-none/release/rtld-relative-rust

if ! "$readelf" -h "$pie" | grep -q 'Type:.*DYN'; then
    printf 'rtld-relative-rust: target is not ET_DYN\n' >&2
    exit 1
fi
if ! "$readelf" -l "$pie" |
    grep -q 'Requesting program interpreter: /libexec/ld-elf.so.1'; then
    printf 'rtld-relative-rust: target has no PT_INTERP ld-elf.so.1\n' >&2
    exit 1
fi
if ! "$readelf" -d "$pie" | grep -q 'NEEDED.*libc.so.1'; then
    printf 'rtld-relative-rust: target does not need libc.so.1\n' >&2
    exit 1
fi
if "$readelf" --dyn-syms "$pie" | grep -qw main; then
    printf 'rtld-relative-rust: target unexpectedly exports main\n' >&2
    exit 1
fi
relative=$("$readelf" -r "$pie" | grep -c 'R_X86_64_RELATIVE' || true)
if [ "$relative" -lt 1 ]; then
    printf 'rtld-relative-rust: target has no R_X86_64_RELATIVE relocation\n' >&2
    exit 1
fi

# Populate the profile's loader/libc/libplug and the C dynamic fixture set.
sh tests/libc/target/build.sh dynamic x86_64

DYNAMIC_EXEC_IMAGE="$pie" \
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build -p raven-dynamic-exec --target x86_64-unknown-none --release
cp "$target_dir/x86_64-unknown-none/release/raven-dynamic-exec-init" \
    "$root/build/libc-test/libc-test"
