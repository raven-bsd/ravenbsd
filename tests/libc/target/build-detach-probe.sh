#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85d: build the detached-thread isolation probe into build/libc-test/ for the libc-test
# profile. It is a diagnostic stress fixture, not a new C73 conformance case.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: arch not in S85d\n'
    exit 77
fi

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
for file in "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
    "$sysroot/usr/lib/crtn.o" "$sysroot/usr/lib/libc.a"; do
    if [ ! -e "$file" ]; then
        printf 'skipped: missing sysroot file %s\n' "$file"
        exit 77
    fi
done

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'skipped: %s not found\n' "$CC"
    exit 77
fi
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
    printf 'skipped: ld.lld not found\n'
    exit 77
fi

out=$root/build/libc-test
mkdir -p "$out/obj"
object=$out/obj/detach-probe.o
source=$root/tests/libc/target/detach-probe.c
fp_flags="-mno-sse -mno-mmx -msoft-float"

"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 $fp_flags -c "$source" -o "$object"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
else
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
fi

: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
