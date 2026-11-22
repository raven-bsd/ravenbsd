#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K502/K534 ASLR probe as the libc-test profile's dynamic PIE /bin/libc-test for the
# selected architecture. The generic dynamic case stages the loader, libc and plug-in; this script
# then replaces the test binary with tests/fixtures/aslr/aslr.c. The qtest profile already runs
# /bin/libc-test as init.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64|aarch64) ;;
    *)
        printf 'skipped: aslr probe supports x86_64 and aarch64 (got %s)\n' "$arch"
        exit 77
        ;;
esac

# Reuse the libc-test dynamic build for the loader, libc, plug-in and the arm64 soft-float ABI.
sh tests/libc/target/build.sh dynamic "$arch"

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

# The aarch64 libc is built for aarch64-unknown-none-softfloat; keep the C probe on the same
# general-register ABI. x86-64 keeps the conservative no-SSE vector used by the other C cases.
if [ "$arch" = aarch64 ]; then
    fp_flags=-mgeneral-regs-only
    linker_flavor="-Wl,-m,aarch64linux"
else
    fp_flags="-mno-sse -mno-mmx -msoft-float"
    linker_flavor=
fi

mkdir -p "$out/obj"
"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 $fp_flags -fPIE \
    -I "$root/libc/ravenlibc/tests/c" \
    -c tests/fixtures/aslr/aslr.c -o "$out/obj/aslr.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/aslr.o" \
        "$out/obj/tls-stub.o" -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
else
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/aslr.o" \
        "$out/obj/tls-stub.o" -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
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
