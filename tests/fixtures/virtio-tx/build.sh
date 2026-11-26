#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K382 build the guest TX-ring stress client and pack a test-only initfs. The base is the
# existing `server` profile (virtio-net, DHCP 10.0.2.15, netd, no shell/brush); this script only
# overlays `/bin/virtio-tx` and its service manifest. No profile or product source is changed.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/virtio-tx
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "virtio-tx fixture: x86_64 only" >&2
    exit 77
fi

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" virtio-tx

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "virtio-tx fixture: $CC not found" >&2
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
    echo "virtio-tx fixture: ld.lld not found" >&2
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
    -c "$root/tests/fixtures/virtio-tx/main.c" -o "$out/main.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/virtio-tx"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/main.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/virtio-tx"
fi

cargo xtask initfs --arch "$arch" --profile server > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

if [ "${VIRTIO_TX_MUTATION:-0}" = "1" ]; then
    drivers=$root/tests/fixtures/virtio-tx/drivers-mutation.toml
else
    drivers=$root/profiles/server.d/drivers.conf
fi

cargo build -q -p raven-b70-overlay
target_dir=${CARGO_TARGET_DIR:-$root/target}
overlay=$target_dir/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/virtio-tx=$out/virtio-tx:0755" \
    "/etc/services.d/virtio-tx.toml=$root/tests/fixtures/virtio-tx/virtio-tx.toml" \
    "/etc/drivers.toml=$drivers"

echo "virtio-tx fixture: staged $out/initfs.img"
