#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K435 SSH kernel-contract guest suite and pack build/fixtures/ssh-contract/initfs.img.
# The image is the network-config-test profile (virtio-net, netd static 10.0.2.15) with the ptyd
# scheme and the one-shot contract services overlaid.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

# A previous hung qtest may still hold the K6 hostfwd port. The suite owns these artifacts;
# clear the old host peer before rebuilding. Do not kill QEMU here: the qtest harness may have
# already started its own instance before running the pre commands.
pkill -f "$root/tests/smp/ssh-contract/host_accept.py" 2>/dev/null || true

out=${SSH_CONTRACT_OUT:-build/fixtures/ssh-contract}
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "ssh-contract fixture: x86_64 only" >&2
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
        echo "ssh-contract fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "ssh-contract fixture: $CC not found" >&2
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
    echo "ssh-contract fixture: ld.lld not found" >&2
    exit 77
fi

link_script=$root/tests/libc/target/x86_64.ld
builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

for source in main child; do
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$root/tests/smp/ssh-contract/$source.c" -o "$out/$source.o"
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

link_one "$out/main.o" "$out/ssh-contract"
link_one "$out/child.o" "$out/ssh-contract-child"

# The diagnostic binary always runs the K1/K6 50x loops, so service args are not involved.
"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -DSSH_CONTRACT_DIAG_BINARY=1 \
    -c "$root/tests/smp/ssh-contract/main.c" -o "$out/main-diag.o"
link_one "$out/main-diag.o" "$out/ssh-contract-diag"

# Always rebuild the two servers: a fixture must not copy a stale ptyd that predates the
# current `pty:ptmx` handler. Cargo is incremental when the sources are unchanged.
for tool in raven-init raven-ptyd; do
    cargo xtask build user --arch "$arch" -p "$tool"
done
init=$root/build/$arch/user/raven-init
ptyd=$root/build/$arch/user/raven-ptyd

cargo xtask initfs --arch "$arch" --profile network-config-test > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

service=$root/tests/smp/ssh-contract/ssh-contract.service.toml
if [ "${SSH_CONTRACT_DIAG:-0}" = "1" ]; then
    service=$root/tests/smp/ssh-contract/ssh-contract-diag.service.toml
fi

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
extra=
if [ "${SSH_CONTRACT_TX_DEBUG:-0}" = "1" ]; then
    extra="/etc/services.d/netd.toml=$root/tests/smp/ssh-contract/netd-tx-debug.toml"
fi
# shellcheck disable=SC2086
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/init=$init" \
    "/bin/ssh-contract=$out/ssh-contract:0755" \
    "/bin/ssh-contract-diag=$out/ssh-contract-diag:0755" \
    "/bin/ssh-contract-child=$out/ssh-contract-child:0755" \
    "/sbin/ptyd=$ptyd:0755" \
    "/etc/services.d/ssh-contract.toml=$service" \
    "/etc/services.d/ptyd.toml=$root/tests/smp/ssh-contract/ptyd.service.toml" \
    "/etc/svcd.conf=$root/tests/smp/ssh-contract/ssh-contract.svcd.conf" \
    $extra

echo "ssh-contract fixture: $out/initfs.img"
