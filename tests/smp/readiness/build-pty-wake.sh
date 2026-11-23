#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K495 rev156: build the pty-master ppoll/SIGCHLD discriminator and pack
# build/smp/readiness/pty-wake/initfs.img. The image is network-config-test with ptyd and the
# one-shot readiness-pty service overlaid.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/smp/readiness/pty-wake
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "readiness pty-wake fixture: x86_64 only" >&2
    exit 77
fi

sysroot=$root/build/$arch/sysroot
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "readiness pty-wake fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "readiness pty-wake fixture: $CC not found" >&2
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
    echo "readiness pty-wake fixture: ld.lld not found" >&2
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
    -c "$root/tests/smp/readiness/pty-wake.c" -o "$out/pty-wake.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/pty-wake.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/pty-wake"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/pty-wake.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/pty-wake"
fi

for tool in raven-init raven-ptyd; do
    # Always rebuild the supervisor binaries. The fixture previously reused an existing
    # binary, so a fix/mutation run could test a stale ptyd and invert the rows.
    cargo xtask build user --arch "$arch" -p "$tool"
done
init=$root/build/$arch/user/raven-init
ptyd=$root/build/$arch/user/raven-ptyd

cargo xtask initfs --arch "$arch" --profile network-config-test > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/init=$init" \
    "/bin/readiness-pty=$out/pty-wake:0755" \
    "/sbin/ptyd=$ptyd:0755" \
    "/etc/services.d/readiness-pty.toml=$root/tests/smp/readiness/readiness-pty.service.toml" \
    "/etc/services.d/ptyd.toml=$root/tests/smp/readiness/ptyd.service.toml" \
    "/etc/svcd.conf=$root/tests/smp/readiness/pty-wake.svcd.conf"

echo "readiness pty-wake fixture: $out/initfs.img"
