#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S183: build the static std TLS-teardown discriminator and an initfs that runs it as /init.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
mode=${2:-normal}
if [ "$arch" != x86_64 ]; then
    printf 'tls-teardown: only x86_64 is in this ticket\n' >&2
    exit 2
fi

archive=$root/build/x86_64/libc.a
if [ ! -f "$archive" ]; then
    printf 'tls-teardown: missing %s; run cargo xtask libc-pack --arch x86_64\n' "$archive" >&2
    exit 2
fi

# The libc-test profile stages /bin/libc-test as the one-shot service, which gives the std
# program the console fds that /init does not have.
out=$root/build/libc-test
mkdir -p "$out"

case "$mode" in
    normal) mode_flags= ;;
    destructor) mode_flags="--cfg tls183_destructor_read" ;;
    *) printf 'tls-teardown: unknown mode %s\n' "$mode" >&2; exit 2 ;;
esac

RUSTUP_TOOLCHAIN=ravenbsd rustc --target x86_64-unknown-ravenbsd --edition 2024 -O \
    $mode_flags \
    -C relocation-model=static -C code-model=small -C strip=symbols \
    -C link-arg=-T"$root/tests/libc/tls/x86_64.ld" \
    -C link-arg="$root/build/x86_64/crt1.o" \
    -C link-arg="$root/build/x86_64/crti.o" \
    -C link-arg="-L$root/build/x86_64" \
    -C link-arg=-lc \
    -C link-arg="$root/build/x86_64/crtn.o" \
    "$root/tests/libc/tls/teardown.rs" -o "$out/libc-test"

: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"
printf 'tls-teardown\n' > "$out/stamp"
file "$out/libc-test"
readelf -h "$out/libc-test" | sed -n '1,12p'
printf 'tls-teardown: staged %s\n' "$out/libc-test"
