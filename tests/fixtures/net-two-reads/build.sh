#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K232 net-two-reads fixture and pack a test-only initfs. The base is the
# network-config-test profile; the fixture overlays /bin/net-two-reads, its service and a
# test-only svcd.conf.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/net-two-reads
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "net-two-reads fixture: x86_64 only" >&2
    exit 77
fi

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" net-two-reads

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "net-two-reads fixture: $CC not found" >&2
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
    echo "net-two-reads fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/fixtures/net-two-reads/main.c" -o "$out/main.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/net-two-reads"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/net-two-reads"
fi

cargo xtask initfs --arch "$arch" --profile network-config-test > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/net-two-reads=$out/net-two-reads:0755" \
    "/etc/services.d/net-two-reads.toml=$root/tests/fixtures/net-two-reads/net-two-reads.toml" \
    "/etc/svcd.conf=$root/tests/fixtures/net-two-reads/svcd.conf"

"$overlay" "$out/base.img" "$out/initfs-exec.img" \
    "/bin/net-two-reads=$out/net-two-reads:0755" \
    "/bin/net-two-reads-exec=$out/net-two-reads:0755" \
    "/bin/net-two-reads-reader=$out/net-two-reads:0755" \
    "/etc/services.d/net-two-reads-exec.toml=$root/tests/fixtures/net-two-reads/net-two-reads-exec.toml" \
    "/etc/svcd.conf=$root/tests/fixtures/net-two-reads/svcd.conf"

echo "net-two-reads fixture: staged $out/initfs.img and $out/initfs-exec.img"
