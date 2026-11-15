#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the C64e Rust std filesystem probe into the libc-test profile's
# `/bin/libc-test` slot. The probe is linked against the freshly packed
# ravenlibc archive; it is executed by the target, not on the host.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'fs-probe: only x86_64 is in C64e\n' >&2
    exit 2
fi

archive=$root/build/x86_64/libc.a
if [ ! -f "$archive" ]; then
    printf 'fs-probe: missing %s; run cargo xtask libc-pack --arch x86_64\n' "$archive" >&2
    exit 2
fi

out=$root/build/libc-test
mkdir -p "$out"
rustc +ravenbsd --target x86_64-unknown-ravenbsd --edition 2024 -O \
    -C relocation-model=static -C link-arg=-no-pie \
    -C link-arg=-T"$root/tests/libc/std-link/std-link.ld" \
    "$root/tests/libc/std-link/fs-probe.rs" -o "$out/libc-test" \
    -C link-arg="$root/build/x86_64/crt1.o" \
    -C link-arg="$root/build/x86_64/crti.o" \
    -C link-arg="-L$root/build/x86_64" \
    -C link-arg=-lc \
    -C link-arg="$root/build/x86_64/crtn.o"

# The libc-test profile lists five generated files. The probe is static, so the
# shared library, loader, child and plug are zero-byte placeholders.
: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"

printf 'fs-probe: %s\n' "$out/libc-test"
file "$out/libc-test"
