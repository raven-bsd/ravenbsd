#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K213 itimer target fixture and pack build/fixtures/itimer/initfs.img. The fixture is a
# static C program linked against the ravenlibc sysroot and packed as /init.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-ravenbsd
        linker_script=$root/tests/libc/target/x86_64.ld
        fp_flags="-mno-sse -mno-mmx -msoft-float"
        linker_flavor=
        ;;
    aarch64)
        target=aarch64-unknown-ravenbsd
        linker_script=$root/userland/linker/aarch64.ld
        fp_flags=-mgeneral-regs-only
        linker_flavor="-Wl,-m,aarch64linux"
        ;;
    *)
        echo "itimer fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/itimer
mkdir -p "$out"

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
# K441: a present stamp must match the tree; a stale artifact fails closed here too.
if [ -f "$root/build/$arch/libc.stamp" ]; then
    cargo xtask libc-pack --arch "$arch" --verify || exit 1
fi
if [ -f "$root/build/$arch/libc-dso.stamp" ]; then
    cargo xtask libc-link --arch "$arch" --verify || exit 1
fi
for file in \
    "$sysroot/usr/lib/crt1.o" \
    "$sysroot/usr/lib/crti.o" \
    "$sysroot/usr/lib/crtn.o" \
    "$sysroot/usr/lib/libc.a"; do
    if [ ! -e "$file" ]; then
        echo "itimer fixture: missing $file (run cargo xtask libc-pack --arch $arch)" >&2
        exit 1
    fi
done

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    echo "itimer fixture: $CC not found" >&2
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
    echo "itimer fixture: ld.lld not found" >&2
    exit 1
fi

"$CC" --target="$target" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 $fp_flags \
    -I "$root/libc/ravenlibc/tests/c" \
    -c tests/qtest/cases/itimer-fixture/itimer.c -o "$out/itimer.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

if [ -n "$builtins" ]; then
    "$CC" --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$linker_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/itimer.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/itimer"
else
    "$CC" --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$linker_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/itimer.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/itimer"
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$out/itimer"
