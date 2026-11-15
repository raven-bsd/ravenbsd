#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the C64d Rust std link probe against the freshly packed ravenlibc
# archive. The probe is a link-only gate; it is not executed here.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'std-link: only x86_64 is in C64d\n' >&2
    exit 2
fi

archive=$root/build/x86_64/libc.a
if [ ! -f "$archive" ]; then
    printf 'std-link: missing %s; run cargo xtask libc-pack --arch x86_64\n' "$archive" >&2
    exit 2
fi

out=$root/build/std-link
mkdir -p "$out"
rustc +ravenbsd --target x86_64-unknown-ravenbsd --edition 2024 -O \
    "$root/tests/libc/std-link/probe.rs" -o "$out/probe" \
    -C link-arg="$root/build/x86_64/crt1.o" \
    -C link-arg="$root/build/x86_64/crti.o" \
    -C link-arg="-L$root/build/x86_64" \
    -C link-arg=-lc \
    -C link-arg="$root/build/x86_64/crtn.o"
printf 'std-link: %s\n' "$out/probe"
file "$out/probe"
if command -v llvm-readelf >/dev/null 2>&1; then
    llvm-readelf -h "$out/probe" | sed -n '1,12p'
else
    readelf -h "$out/probe" | sed -n '1,12p'
fi
