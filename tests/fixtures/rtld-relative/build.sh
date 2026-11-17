#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C74l regression fixture: a minimal C PIE with one R_X86_64_RELATIVE relocation.
#
# The image is embedded in the existing static dynamic-exec launcher and staged as the
# libc-test profile's /bin/libc-test program. The loader must apply the relative relocation
# before the program reaches main.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: rtld-relative is x86_64 only\n'
    exit 77
fi

out=build/fixtures/rtld-relative
mkdir -p "$out"

sysroot=${RAVENLIBC_SYSROOT:-$root/build/x86_64/sysroot}
if [ ! -f "$sysroot/libexec/ld-elf.so.1" ] ||
    [ ! -f "$sysroot/lib/libc.so.1" ] ||
    [ ! -f "$sysroot/usr/lib/Scrt1.o" ]; then
    cargo xtask libc-pack --arch x86_64
    cargo xtask libc-link --arch x86_64
fi

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'rtld-relative: %s not found\n' "$CC" >&2
    exit 1
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
    printf 'rtld-relative: ld.lld not found\n' >&2
    exit 1
fi

readelf=${READELF:-}
if [ -z "$readelf" ]; then
    if command -v llvm-readelf >/dev/null 2>&1; then
        readelf=llvm-readelf
    else
        readelf=readelf
    fi
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -fPIE -D__ravenbsd__=1 -D__unix__=1 \
    -c tests/fixtures/rtld-relative/relative.c -o "$out/relative.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/relative.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/relative"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/relative.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/relative"
fi

if ! "$readelf" -h "$out/relative" | grep -q 'Type:.*DYN'; then
    printf 'rtld-relative: target is not ET_DYN\n' >&2
    exit 1
fi
if ! "$readelf" -l "$out/relative" |
    grep -q 'Requesting program interpreter: /libexec/ld-elf.so.1'; then
    printf 'rtld-relative: target has no PT_INTERP ld-elf.so.1\n' >&2
    exit 1
fi
if ! "$readelf" -d "$out/relative" | grep -q 'NEEDED.*libc.so.1'; then
    printf 'rtld-relative: target does not need libc.so.1\n' >&2
    exit 1
fi
if [ "$("$readelf" -r "$out/relative" | grep -c 'R_X86_64_RELATIVE')" -lt 1 ]; then
    printf 'rtld-relative: target has no R_X86_64_RELATIVE relocation\n' >&2
    exit 1
fi

# Populate build/libc-test with the profile's loader, libc and helper files.
sh tests/libc/target/build.sh dynamic x86_64

DYNAMIC_EXEC_IMAGE="$root/$out/relative" \
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-dynamic-exec --target x86_64-unknown-none --release
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
cp "$target/raven-dynamic-exec-init" "$root/build/libc-test/libc-test"
