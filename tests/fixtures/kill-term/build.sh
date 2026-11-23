#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 kill(SIGTERM) console fixture. Builds the binary staged at /bin/kill-term
# by profiles/release.d/93-kill-term.toml. No product source is changed.
#
# Usage: sh tests/fixtures/kill-term/build.sh [x86_64|aarch64]
# Output: build/fixtures/kill-term/<arch>/kill-term

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target=x86_64-unknown-ravenbsd
        link_script=$root/tests/libc/target/x86_64.ld
        fp_flags="-mno-sse -mno-mmx -msoft-float"
        linker_flavor=
        builtins_path=
        ;;
    aarch64)
        target=aarch64-unknown-ravenbsd
        link_script=$root/userland/linker/aarch64.ld
        fp_flags=-mgeneral-regs-only
        linker_flavor=-Wl,-m,aarch64linux
        builtins_path=/usr/lib/gcc-cross/aarch64-linux-gnu/11/libgcc.a
        ;;
    *)
        echo "kill-term: unsupported arch $arch" >&2
        exit 2
        ;;
esac
out=build/fixtures/kill-term/$arch
mkdir -p "$out"

sysroot=$root/build/$arch/sysroot
if [ ! -f "$sysroot/usr/lib/libc.a" ] || [ ! -f "$sysroot/usr/lib/crt1.o" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "kill-term: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "kill-term: $CC not found" >&2
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
    echo "kill-term: ld.lld not found" >&2
    exit 77
fi

builtins_arg=
if [ -n "$builtins_path" ]; then
    builtins_arg=$builtins_path
elif [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins_arg=-lclang_rt.builtins
fi
"$CC" --target="$target" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 $fp_flags \
    -c "$root/tests/fixtures/kill-term/kill_term.c" -o "$out/kill_term.o"

if [ -n "$builtins_arg" ]; then
    "$CC" --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/kill_term.o" \
        -L"$sysroot/usr/lib" -lc "$builtins_arg" \
        "$sysroot/usr/lib/crtn.o" -o "$out/kill-term"
else
    "$CC" --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/kill_term.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/kill-term"
fi

echo "kill-term: staged $out/kill-term"
