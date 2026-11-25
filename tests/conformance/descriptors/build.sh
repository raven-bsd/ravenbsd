#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S172: build the descriptor conformance probe against ravenlibc.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=$root/build/conformance/descriptors
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "descriptor-conformance: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

CC=${CC:-clang}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    echo "descriptor-conformance: ld.lld not found" >&2
    exit 1
fi

common="-std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration"
common="$common -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float"
common="$common --target=$arch-unknown-ravenbsd --sysroot=$sysroot"

"$CC" $common -c "$root/tests/conformance/harness/conformance.c" \
    -o "$out/conformance.o"
"$CC" $common -I"$root/tests/conformance/harness" \
    -c "$root/tests/conformance/descriptors/descriptor-conformance.c" \
    -o "$out/descriptor-conformance.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

if [ -n "$builtins" ]; then
    "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
        "$out/descriptor-conformance.o" "$out/conformance.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/conformance-probe"
else
    "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
        "$out/descriptor-conformance.o" "$out/conformance.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/conformance-probe"
fi

sh "$root/tests/libc/target/shape-check.sh" "$out/conformance-probe"
echo "descriptor-conformance: staged $out/conformance-probe"
