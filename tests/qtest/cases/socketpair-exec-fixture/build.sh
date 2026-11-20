#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K195 socketpair-exec fixture and pack build/fixtures/socketpair-exec/initfs.img.
# The fixture links the real libc.a: pair, fork, child writes 563+191 bytes, parent dup2s the
# peer to fd 4 and execve()s the static reader. This is the OpenSSH rexec fd shape.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/socketpair-exec
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "socketpair-exec fixture: only x86_64 is supported" >&2
    exit 1
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
        echo "socketpair-exec fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socketpair-exec fixture: $CC not found" >&2
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

for name in main reader; do
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$root/tests/qtest/cases/socketpair-exec-fixture/$name.c" -o "$out/$name.o"
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$name.o" \
        -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" \
        -o "$out/socketpair-exec-$name"
done

init=$root/build/$arch/user/raven-init
if [ ! -x "$init" ]; then
    cargo xtask build user --arch "$arch" -p raven-init
fi

# The fixture needs the `file:` scheme so execve() can read /bin/socketpair-reader. Pack the
# minimal service chain (logd -> ramfsd/nulld/zerod/randd -> initfsd -> vfsd), matching the base
# profile's manifests.
for name in logd ramfsd nulld zerod randd initfsd vfsd; do
    binary=$root/build/$arch/user/raven-$name
    [ -x "$binary" ] || {
        echo "socketpair-exec fixture: missing $binary" >&2
        exit 1
    }
done

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$init" \
    "/bin/socketpair-exec=$out/socketpair-exec-main" \
    "/bin/socketpair-reader=$out/socketpair-exec-reader" \
    "/sbin/logd=$root/build/$arch/user/raven-logd" \
    "/sbin/ramfsd=$root/build/$arch/user/raven-ramfsd" \
    "/sbin/nulld=$root/build/$arch/user/raven-nulld" \
    "/sbin/zerod=$root/build/$arch/user/raven-zerod" \
    "/sbin/randd=$root/build/$arch/user/raven-randd" \
    "/sbin/initfsd=$root/build/$arch/user/raven-initfsd" \
    "/sbin/vfsd=$root/build/$arch/user/raven-vfsd" \
    "/etc/services.d/logd.toml=$root/etc/services.d/logd.toml" \
    "/etc/services.d/ramfsd.toml=$root/etc/services.d/ramfsd.toml" \
    "/etc/services.d/nulld.toml=$root/etc/services.d/nulld.toml" \
    "/etc/services.d/zerod.toml=$root/etc/services.d/zerod.toml" \
    "/etc/services.d/randd.toml=$root/etc/services.d/randd.toml" \
    "/etc/services.d/initfsd.toml=$root/etc/services.d/initfsd.toml" \
    "/etc/services.d/vfsd.toml=$root/etc/services.d/vfsd.toml" \
    "/etc/services.d/socketpair-exec.toml=$root/tests/qtest/cases/socketpair-exec.service.toml" \
    "/etc/svcd.conf=$root/tests/qtest/cases/socketpair-exec.svcd.conf"

echo "socketpair-exec fixture: $out/initfs.img"
