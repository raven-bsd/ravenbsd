#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S175 test-only: build the /bin/terminal-conformance probe for the release image.
#
# Usage: sh tests/conformance/terminal/build.sh
# Output: build/conformance/terminal/terminal-conformance

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/conformance/terminal
mkdir -p "$out"
arch=x86_64
sysroot=$root/build/$arch/sysroot

# Refresh the static sysroot from the current libc sources (the same step the libc cases use).
sh tests/libc/target/build.sh termios >/dev/null 2>&1 || true

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "terminal-conformance: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

CC=${CC:-clang}
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
    -c "$root/tests/conformance/terminal/terminal-conformance.c" \
    -o "$out/terminal-conformance.o"

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/terminal-conformance.o" \
    -L"$sysroot/usr/lib" -lc \
    "$sysroot/usr/lib/crtn.o" -o "$out/terminal-conformance"

echo "terminal-conformance: staged $out/terminal-conformance"
