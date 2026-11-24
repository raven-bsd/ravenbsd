#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K231 exec'd accepted-socket fixture and pack a test-only initfs. The base image is
# the `network-config-test` profile (virtio-net, netd static 10.0.2.15, ravenlibc); the fixture
# overlays `/bin/socket-inet-exec` (the forking listener), `/bin/socket-inet-exec-child` (the
# exec'd session), the service manifest and a test-only `/etc/svcd.conf`.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-inet-exec
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "socket-inet-exec fixture: x86_64 only" >&2
    exit 77
fi

# The fixture links the libc archive the image runs: rebuild it when any libc source is newer, so
# a libc change is never tested against a stale archive.
sysroot=$root/build/$arch/sysroot
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] || [ ! -f "$sysroot/usr/lib/crt1.o" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "socket-inet-exec fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socket-inet-exec fixture: $CC not found" >&2
    exit 77
}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    echo "socket-inet-exec fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

build_binary() {
    source=$1
    object=$2
    binary=$3

    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$source" -o "$object"
    if [ -n "$builtins" ]; then
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$binary"
    else
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$binary"
    fi
}

build_binary "$root/tests/fixtures/socket-inet-exec/main.c" "$out/main.o" "$out/socket-inet-exec"
build_binary "$root/tests/fixtures/socket-inet-exec/child.c" "$out/child.o" \
    "$out/socket-inet-exec-child"

cargo xtask initfs --arch "$arch" --profile network-config-test > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/socket-inet-exec=$out/socket-inet-exec:0755" \
    "/bin/socket-inet-exec-child=$out/socket-inet-exec-child:0755" \
    "/etc/services.d/socket-inet-exec.toml=$root/tests/fixtures/socket-inet-exec/socket-inet-exec.toml" \
    "/etc/svcd.conf=$root/tests/fixtures/socket-inet-exec/svcd.conf"

echo "socket-inet-exec fixture: staged $out/initfs.img"
