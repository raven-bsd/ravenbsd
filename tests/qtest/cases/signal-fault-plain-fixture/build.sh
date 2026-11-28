#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S779 plain signal(2) fixtures against the generated libc sysroot.
#
# Two images share one source: `signal-fault-plain` uses the donor `mprotect_write` flags
# (`MAP_ANON` with no private/shared type) and `signal-fault-plain-private` adds `MAP_PRIVATE`
# so the plain-handler path is exercised while the donor mapping is unavailable. Test-only
# code; the product build is Rust.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) ;;
    *)
        echo "signal-fault-plain fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
if [ ! -f "$sysroot/usr/lib/libc.a" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtbegin.o crtend.o crtn.o libc.a; do
    if [ ! -e "$sysroot/usr/lib/$file" ]; then
        echo "signal-fault-plain fixture: missing sysroot file $file" >&2
        exit 77
    fi
done

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    echo "signal-fault-plain fixture: $CC not found" >&2
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
    echo "signal-fault-plain fixture: ld.lld not found" >&2
    exit 77
fi

build() {
    name=$1
    extra=$2
    out=build/fixtures/$name
    mkdir -p "$out"
    object=$out/main.o
    # shellcheck disable=SC2086
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float $extra \
        -c tests/qtest/cases/signal-fault-plain-fixture/src/main.c -o "$object"
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -Ttests/libc/target/x86_64.ld \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
        "$sysroot/usr/lib/crtbegin.o" "$object" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtend.o" "$sysroot/usr/lib/crtn.o" -o "$out/init"
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$out/initfs.img" \
        /init="$out/init"
}

build signal-fault-plain ""
build signal-fault-plain-private "-DS779_PRIVATE_MAP"
