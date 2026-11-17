#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S81 dynamic-exec fixture:
#   /init                 static launcher that calls PROC_EXEC on /bin/dynamic-exec
#   /bin/dynamic-exec     normally linked ET_DYN with PT_INTERP and DT_NEEDED libc.so.1
#   /libexec/ld-elf.so.1  the PT_INTERP image
#   /lib/libc.so.1        the DT_NEEDED library
#
# Nothing here invokes `ld-elf.so.1 <program>`: the kernel must load both images and start the
# interpreter. The shape checks below fail the fixture if the target binary is not dynamic.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: dynamic-exec is x86_64 only in S81\n'
    exit 77
fi

out=build/fixtures/dynamic-exec
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
    printf 'dynamic-exec: %s not found\n' "$CC" >&2
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
    printf 'dynamic-exec: ld.lld not found\n' >&2
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
    -c tests/fixtures/dynamic-exec/dynamic-exec.c -o "$out/dynamic-exec.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/dynamic-exec.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/dynamic-exec"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/dynamic-exec.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/dynamic-exec"
fi

if ! "$readelf" -h "$out/dynamic-exec" | grep -q 'Type:.*DYN'; then
    printf 'dynamic-exec: target is not ET_DYN\n' >&2
    exit 1
fi
if ! "$readelf" -l "$out/dynamic-exec" |
    grep -q 'Requesting program interpreter: /libexec/ld-elf.so.1'; then
    printf 'dynamic-exec: target has no PT_INTERP ld-elf.so.1\n' >&2
    exit 1
fi
if ! "$readelf" -d "$out/dynamic-exec" | grep -q 'NEEDED.*libc.so.1'; then
    printf 'dynamic-exec: target does not need libc.so.1\n' >&2
    exit 1
fi

# Populate build/libc-test with the profile's loader, libc and helper files. The dynamic-exec
# case uses the libc-test profile so the already-tested service stack provides the file scheme
# the loader needs for /lib/libc.so.1.
sh tests/libc/target/build.sh dynamic x86_64

DYNAMIC_EXEC_IMAGE="$root/$out/dynamic-exec" \
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-dynamic-exec --target x86_64-unknown-none --release
target=${CARGO_TARGET_DIR:-target}/x86_64-unknown-none/release
cp "$target/raven-dynamic-exec-init" "$root/build/libc-test/libc-test"
