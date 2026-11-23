#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K445 release-profile fixture: build the release initfs, then overlay only the K1-K6 probe and
# its service so the contract runs against the shipped userland/netd/pty rather than the test
# profile.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=${SSH_CONTRACT_OUT:-build/fixtures/ssh-contract-release}
service=${SSH_CONTRACT_SERVICE:-$root/tests/smp/ssh-contract/ssh-contract.service.toml}
arch=${RAVEN_QT_ARCH:-x86_64}
mkdir -p "$out"
pkill -f "$root/tests/smp/ssh-contract/host_accept.py" 2>/dev/null || true

sysroot=$root/build/$arch/sysroot
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "ssh-contract release: $CC not found" >&2
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

link_script=$root/tests/libc/target/x86_64.ld
for source in main child; do
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$root/tests/smp/ssh-contract/$source.c" -o "$out/$source.o"
done

link_one() {
    object=$1
    binary=$2
    if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
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

link_one "$out/main.o" "$out/ssh-contract"
link_one "$out/child.o" "$out/ssh-contract-child"

cargo xtask initfs --arch "$arch" --profile release > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
"$root/target/debug/raven-b70-overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/ssh-contract=$out/ssh-contract:0755" \
    "/bin/ssh-contract-child=$out/ssh-contract-child:0755" \
    "/etc/services.d/ssh-contract.toml=$service" \
    "/etc/svcd.conf=$root/tests/smp/ssh-contract/ssh-contract.svcd.conf"

echo "ssh-contract release fixture: $out/initfs.img"
