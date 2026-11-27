#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S173b PG-26 pointer fixture and pack build/fixtures/pg26-pointer/initfs.img
# with it as /init. The qtest boots the normal base kernel with this initfs; the fixture
# exercises the real libc ppoll()/USER_COPY path, so no staged binary is reused.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "pg26-pointer fixture: x86_64 only" >&2
    exit 77
fi

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" pg26-pointer

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "pg26-pointer fixture: $CC not found" >&2
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
    echo "pg26-pointer fixture: ld.lld not found" >&2
    exit 77
fi

out=build/fixtures/pg26-pointer
mkdir -p "$out"
link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/fixtures/pg26-pointer/main.c" -o "$out/main.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/pg26-pointer"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/pg26-pointer"
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" "/init=$out/pg26-pointer:0755"
