#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K502 ASLR probe as the libc-test profile's dynamic PIE /bin/libc-test. The generic
# dynamic case stages the loader, libc and plug-in; this script then replaces the test binary with
# tests/fixtures/aslr/aslr.c. The qtest profile already runs /bin/libc-test as init.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: aslr probe is x86_64 only\n'
    exit 77
fi

sh tests/libc/target/build.sh dynamic x86_64

out=$root/build/libc-test
sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
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

mkdir -p "$out/obj"
"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -fPIE \
    -I "$root/libc/ravenlibc/tests/c" \
    -c tests/fixtures/aslr/aslr.c -o "$out/obj/aslr.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/aslr.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/aslr.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
fi

readelf=${READELF:-}
if [ -z "$readelf" ]; then
    if command -v llvm-readelf >/dev/null 2>&1; then
        readelf=llvm-readelf
    else
        readelf=readelf
    fi
fi
if ! "$readelf" -h "$out/libc-test" | grep -q 'Type:.*DYN'; then
    printf 'aslr: probe is not ET_DYN\n' >&2
    exit 1
fi
if ! "$readelf" -l "$out/libc-test" |
    grep -q 'Requesting program interpreter: /libexec/ld-elf.so.1'; then
    printf 'aslr: probe has no PT_INTERP ld-elf.so.1\n' >&2
    exit 1
fi
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
