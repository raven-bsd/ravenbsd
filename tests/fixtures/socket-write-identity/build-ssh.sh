#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K431 S5 identity fixture and pack a test-only initfs. The guest owns the listener, the
# host peer connects over the K194 tap, and the fork+exec child writes sentinels through fd 7/1/2.
# The base image is the lean `server` profile (netd + virtio-net) with the prebuilt first-party
# /bin/rsh staged in, so the exec child can run the shell without a full release/uutils rebuild.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-write-identity
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "socket-write-identity ssh fixture: x86_64 only" >&2
    exit 77
fi

sysroot=$root/build/$arch/sysroot
if [ ! -f "$sysroot/usr/lib/libc.a" ] || [ ! -f "$sysroot/usr/lib/crt1.o" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "socket-write-identity ssh fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "socket-write-identity ssh fixture: $CC not found" >&2
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
    echo "socket-write-identity ssh fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

build_program() {
    name=$1
    source=$2
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$source" -o "$out/$name.o"
    if [ -n "$builtins" ]; then
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$name.o" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$out/$name"
    else
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$name.o" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$out/$name"
    fi
}

build_program socket-write-identity "$root/tests/fixtures/socket-write-identity/ssh_main.c"
build_program socket-write-identity-ssh-child "$root/tests/fixtures/socket-write-identity/ssh_child.c"

cargo xtask initfs --arch "$arch" --profile server > /dev/null
cp build/"$arch"/initfs.img "$out/base-ssh.img"

rsh=$root/build/"$arch"/user/raven-rsh
[ -x "$rsh" ] || {
    echo "socket-write-identity ssh fixture: missing $rsh (build the userland first)" >&2
    exit 1
}
consd=$root/build/"$arch"/user/raven-consd
[ -x "$consd" ] || {
    echo "socket-write-identity ssh fixture: missing $consd (build the userland first)" >&2
    exit 1
}

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
"$overlay" "$out/base-ssh.img" "$out/initfs-ssh.img" \
    "/bin/rsh=$rsh:0755" \
    "/sbin/consd=$consd:0755" \
    "/bin/socket-write-identity=$out/socket-write-identity:0755" \
    "/bin/socket-write-identity-ssh-child=$out/socket-write-identity-ssh-child:0755" \
    "/etc/raven/conf.d/network.toml=$root/tests/fixtures/socket-write-identity/network-static.toml" \
    "/etc/services.d/consd.toml=$root/tests/fixtures/socket-write-identity/consd.service.manifest" \
    "/etc/services.d/socket-write-identity.toml=$root/tests/fixtures/socket-write-identity/socket-write-identity-ssh.toml" \
    "/etc/svcd.conf=$root/tests/fixtures/socket-write-identity/svcd.conf"

echo "socket-write-identity ssh fixture: staged $out/initfs-ssh.img"
