#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K408 build the guest socket-write fixture and a test-only initfs. The base is the existing
# `server` profile (virtio-net, DHCP, netd, no shell); this script overlays the parent and the
# fork+exec child plus their service manifest. No profile or product source is changed.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-write-exec
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
mode=${SOCKET_WRITE_EXEC_MODE:-connect}
if [ "$arch" != x86_64 ]; then
    echo "socket-write-exec fixture: x86_64 only" >&2
    exit 77
fi
case "$mode" in
connect)
    manifest=socket-write-exec.toml
    image=initfs.img
    ;;
accept)
    manifest=socket-write-exec-tap.toml
    image=initfs-tap.img
    ;;
*)
    echo "socket-write-exec fixture: unknown mode $mode" >&2
    exit 2
    ;;
esac

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" socket-write-exec

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socket-write-exec fixture: $CC not found" >&2
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
    echo "socket-write-exec fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
for name in main child; do
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$root/tests/fixtures/socket-write-exec/$name.c" -o "$out/$name.o"
done

link_one() {
    object=$1
    binary=$2
    if [ -n "$builtins" ]; then
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$binary"
    else
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$binary"
    fi
}
link_one "$out/main.o" "$out/socket-write-exec"
link_one "$out/child.o" "$out/socket-write-exec-child"

cargo xtask initfs --arch "$arch" --profile server > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
target_dir=${CARGO_TARGET_DIR:-$root/target}
overlay=$target_dir/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/$image" \
    "/bin/socket-write-exec=$out/socket-write-exec:0755" \
    "/bin/socket-write-exec-child=$out/socket-write-exec-child:0755" \
    "/etc/services.d/socket-write-exec.toml=$root/tests/fixtures/socket-write-exec/$manifest"

echo "socket-write-exec fixture: staged $out/$image (mode=$mode)"
