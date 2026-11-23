#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K495 rev174 test-only pty path fixture. Build the /bin/pty-path probe used by the test-only
# pty-path image overlay. No product source is changed.
#
# Usage: sh tests/fixtures/pty-path/build.sh
# Output: build/fixtures/pty-path/pty-path

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pty-path
mkdir -p "$out"
arch=x86_64
sysroot=$root/build/$arch/sysroot
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "pty-path fixture: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "pty-path fixture: $CC not found" >&2
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

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/fixtures/pty-path/pty-path.c" -o "$out/pty-path.o"

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/pty-path.o" \
    -L"$sysroot/usr/lib" -lc \
    "$sysroot/usr/lib/crtn.o" -o "$out/pty-path"

echo "pty-path fixture: staged $out/pty-path"
