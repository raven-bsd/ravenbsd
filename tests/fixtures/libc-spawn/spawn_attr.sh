#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P9c attribute/file-action spawn probe into build/libc-test/libc-test, after the
# C73 driver has staged libc-child and the sysroot placeholders.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

sh tests/libc/target/build.sh spawn x86_64

sysroot=${RAVENLIBC_SYSROOT:-$root/build/x86_64/sysroot}
out=$root/build/libc-test
linker=$root/tests/libc/target/x86_64.ld
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

case $sysroot in
    /*) ;;
    *) sysroot=$root/$sysroot ;;
esac

if [ ! -f "$sysroot/usr/lib/libc.a" ]; then
    printf 'spawn-attr: missing %s/usr/lib/libc.a\n' "$sysroot" >&2
    exit 1
fi

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

mkdir -p "$out/obj"
"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -I "$root/libc/ravenlibc/tests/c" \
    -c "$root/tests/fixtures/libc-spawn/spawn_attr.c" -o "$out/obj/spawn_attr.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$linker" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/spawn_attr.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$linker" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/spawn_attr.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
fi
