#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 S0 build the guest echo fixture into a throw-away `server` initfs. The server profile
# (virtio-net, DHCP, netd) is overlaid with /bin/ssh-echo and its service manifest; no release
# profile or product source is changed.
#
# Usage: sh tests/fixtures/ssh-echo/build.sh
# Output: build/fixtures/ssh-echo/echo.img

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/ssh-echo
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "ssh-echo fixture: x86_64 only" >&2
    exit 77
fi

sysroot=$root/build/$arch/sysroot
if [ ! -f "$sysroot/usr/lib/libc.a" ] || [ ! -f "$sysroot/usr/lib/crt1.o" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "ssh-echo fixture: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "ssh-echo fixture: $CC not found" >&2
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
    echo "ssh-echo fixture: ld.lld not found" >&2
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
    -c "$root/tests/fixtures/ssh-echo/echo.c" -o "$out/echo.o"

if [ -n "$builtins" ]; then
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/echo.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/ssh-echo"
else
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/echo.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/ssh-echo"
fi

cargo xtask initfs --arch "$arch" --profile server > /dev/null
cp build/"$arch"/initfs.img "$out/base.img"

cargo build -q -p raven-b70-overlay
target_dir=${CARGO_TARGET_DIR:-$root/target}
overlay=$target_dir/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/ssh-echo=$out/ssh-echo:0755" \
    "/etc/services.d/ssh-echo.toml=$root/tests/fixtures/ssh-echo/ssh-echo.service.manifest"

cargo xtask image --arch "$arch" --profile server --initfs "$out/initfs.img" \
    --output "$out/echo.img" > /dev/null

echo "ssh-echo fixture: staged $out/echo.img"
