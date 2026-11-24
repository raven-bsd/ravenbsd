#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S179 conformance probe against the Raven libc sysroot. The probe is a static
# x86_64-unknown-ravenbsd executable staged at /bin/cred-probe by tests/conformance/credentials/stage.py.
# No product source is changed.
#
# Usage: sh tests/conformance/credentials/probe/build.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/credentials
mkdir -p "$out"

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" "cred-probe"

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "cred-probe: $CC not found" >&2
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
    echo "cred-probe: ld.lld not found" >&2
    exit 77
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c tests/conformance/credentials/probe/cred_probe.c -o "$out/cred-probe.o"

if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins_arg=-lclang_rt.builtins
else
    builtins_arg=
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T tests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/cred-probe.o" \
    -L"$sysroot/usr/lib" -lc $builtins_arg \
    "$sysroot/usr/lib/crtn.o" -o "$out/cred-probe"

echo "cred-probe: built $out/cred-probe"
