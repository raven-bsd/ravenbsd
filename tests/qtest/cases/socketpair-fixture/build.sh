#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K206 libc socketpair fixture and pack build/fixtures/socketpair/initfs.img.
# The fixture links the real libc.a; it does not call a raw kernel primitive directly.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/socketpair
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "socketpair fixture: only x86_64 is supported" >&2
    exit 1
fi

sysroot=$root/build/$arch/sysroot
# K441: refuse an artifact older than the sources before the mtime-based refresh can hide it.
if [ -f "$root/build/$arch/libc.stamp" ]; then
    cargo xtask libc-pack --arch "$arch" --verify || exit 1
fi
if [ -f "$root/build/$arch/libc-dso.stamp" ]; then
    cargo xtask libc-link --arch "$arch" --verify || exit 1
fi
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "socketpair fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socketpair fixture: $CC not found" >&2
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

"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/qtest/cases/socketpair-fixture/main.c" -o "$out/main.o"

"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
    -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" \
    -o "$out/libc-socketpair"

init=$root/build/$arch/user/raven-init
if [ ! -x "$init" ]; then
    cargo xtask build user --arch "$arch" -p raven-init
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$init" \
    "/bin/libc-socketpair=$out/libc-socketpair" \
    "/etc/services.d/socketpair.toml=$root/tests/qtest/cases/socketpair.service.toml" \
    "/etc/svcd.conf=$root/tests/qtest/cases/socketpair.svcd.conf"

echo "socketpair fixture: $out/initfs.img"
