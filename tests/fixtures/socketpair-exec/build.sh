#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K222 socketpair-exec fixture into the libc-test profile's staging paths.
# `build/libc-test/libc-test` is the driver and `build/libc-test/libc-child` is the static
# reader the driver execs. The profile's initfsd/vfsd services provide the file scheme.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socketpair-exec
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "socketpair-exec fixture: only x86_64 is supported" >&2
    exit 1
fi

sysroot=$root/build/$arch/sysroot
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "socketpair-exec fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

# Populate the profile's loader, libc, helper and service files; the profile initfs is used.
sh tests/libc/target/build.sh dynamic "$arch"

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socketpair-exec fixture: $CC not found" >&2
    exit 1
}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
    if [ -x "$lld_dir/ld.lld" ]; then
        PATH="$lld_dir:$PATH"
        export PATH
    fi
fi

build_binary() {
    source=$1
    object=$2
    binary=$3

    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$source" -o "$object"

    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
        -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" \
        -o "$binary"
}

build_binary "$root/tests/fixtures/socketpair-exec/main.c" "$out/main.o" \
    "$root/build/libc-test/libc-test"
build_binary "$root/tests/fixtures/socketpair-exec/reader.c" "$out/reader.o" \
    "$root/build/libc-test/libc-child"

echo "socketpair-exec fixture: staged build/libc-test/libc-test and libc-child"
