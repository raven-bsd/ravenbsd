#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K223 chroot fixture into the libc-test profile's staging paths.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "chroot fixture: x86_64 only" >&2
    exit 77
fi
sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" chroot
CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    echo "chroot fixture: $CC not found" >&2
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
    echo "chroot fixture: ld.lld not found" >&2
    exit 77
fi

stage=$root/build/libc-test
mkdir -p "$stage/obj"
link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

compile() {
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$1" -o "$2"
}

link() {
    if [ -n "$builtins" ]; then
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$1" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$2"
    else
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$1" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$2"
    fi
}

compile "$root/tests/fixtures/chroot/chroot.c" "$stage/obj/chroot.o"
link "$stage/obj/chroot.o" "$stage/libc-test"
compile "$root/tests/fixtures/chroot/chroot-child.c" "$stage/obj/chroot-child.o"
link "$stage/obj/chroot-child.o" "$stage/libc-child"
: > "$stage/libplug.so"
: > "$stage/libc.so.1"
: > "$stage/ld-elf.so.1"
printf 'chroot fixture: %s %s\n' "$stage/libc-test" "$stage/libc-child"
ls -l "$stage/libc-test" "$stage/libc-child"
