#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S733: build the Rust filesystem-record conformance probe against ravenlibc. There is no C/C++
# source: the shared row contract is tests/conformance/harness/conformance.rs and the probe is a
# no_std Rust binary linked with the same crt/libc archive pattern as the descriptor fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=$root/build/conformance/fs-records
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot
linker=$root/userland/linker/x86_64.ld

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "fs-records-conformance: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done
[ -f "$linker" ] || {
    echo "fs-records-conformance: missing linker script $linker" >&2
    exit 1
}

rust_target=x86_64-unknown-none
rustc --target "$rust_target" --edition 2024 -O -C panic=abort \
    -C relocation-model=static -C code-model=small \
    -C link-arg=-static -C link-arg=-T"$linker" \
    -C link-arg="$sysroot/usr/lib/crt1.o" \
    -C link-arg="$sysroot/usr/lib/crti.o" \
    -C link-arg="$sysroot/usr/lib/libc.a" \
    -C link-arg="$sysroot/usr/lib/crtn.o" \
    "$root/tests/conformance/fs-records/fs-records-conformance.rs" \
    -o "$out/conformance-probe"

sh "$root/tests/libc/target/shape-check.sh" "$out/conformance-probe"
echo "fs-records-conformance: staged $out/conformance-probe"
