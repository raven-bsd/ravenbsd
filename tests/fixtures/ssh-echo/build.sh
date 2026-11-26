#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 S0 / rev206 guest echo fixture. The guest owns a TCP listener on the K194 tap; the host
# connects, sends one exact payload, and must receive the same bytes back. The program is test
# tooling only and is staged into a throw-away `server` initfs (x86_64) or a test release profile
# (aarch64), never into the shipped release image.
#
# Usage: sh tests/fixtures/ssh-echo/build.sh [x86_64|aarch64]
# Output: build/fixtures/ssh-echo/<arch>/ssh-echo, and the x86_64 echo.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case $arch in
    x86_64) ;;
    aarch64) ;;
    *) echo "ssh-echo fixture: unsupported arch $arch" >&2; exit 2 ;;
esac
out=build/fixtures/ssh-echo/$arch
mkdir -p "$out"

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

link_script=$root/tests/libc/target/$arch.ld
if [ "$arch" = aarch64 ]; then
    link_script=$root/userland/linker/aarch64.ld
    linker_flavor=-Wl,-m,aarch64linux
fi
[ -f "$link_script" ] || {
    echo "ssh-echo fixture: missing $link_script" >&2
    exit 1
}

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

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
float_flags=
if [ "$arch" = x86_64 ]; then
    float_flags="-mno-sse -mno-mmx -msoft-float"
fi
"$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 $float_flags \
    -c "$root/tests/fixtures/ssh-echo/echo.c" -o "$out/echo.o"

if [ -n "$builtins" ]; then
    "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/echo.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/ssh-echo"
else
    "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$link_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/echo.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/ssh-echo"
fi

if [ "$arch" = x86_64 ]; then
    cargo xtask initfs --arch "$arch" --profile server > /dev/null
    cp build/"$arch"/initfs.img build/fixtures/ssh-echo/base.img
    cargo build -q -p raven-b70-overlay
    target_dir=${CARGO_TARGET_DIR:-$root/target}
    overlay=$target_dir/debug/raven-b70-overlay
    "$overlay" build/fixtures/ssh-echo/base.img build/fixtures/ssh-echo/initfs.img \
        "/bin/ssh-echo=$out/ssh-echo:0755" \
        "/etc/services.d/ssh-echo.toml=$root/tests/fixtures/ssh-echo/ssh-echo.service.manifest"
    cargo xtask image --arch "$arch" --profile server \
        --initfs build/fixtures/ssh-echo/initfs.img \
        --output build/fixtures/ssh-echo/echo.img > /dev/null
    echo "ssh-echo fixture: staged $out/ssh-echo and build/fixtures/ssh-echo/echo.img"
else
    "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 \
        -c "$root/tests/fixtures/ssh-echo/proc-probe.c" -o "$out/proc-probe.o"
    if [ -n "$builtins" ]; then
        "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/proc-probe.o" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$out/proc-probe"
    else
        "$CC" --target=$arch-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -static -T"$link_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/proc-probe.o" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$out/proc-probe"
    fi
    cargo xtask initfs --arch "$arch" --profile release > /dev/null
    cp build/"$arch"/initfs.img "$out/base.img"
    cargo build -q -p raven-b70-overlay
    target_dir=${CARGO_TARGET_DIR:-$root/target}
    overlay=$target_dir/debug/raven-b70-overlay
    "$overlay" "$out/base.img" "$out/initfs.img" \
        "/bin/ssh-echo=$out/ssh-echo:0755" \
        "/bin/proc-probe=$out/proc-probe:0755" \
        "/etc/services.d/ssh-echo.toml=$root/tests/fixtures/ssh-echo/ssh-echo.service.manifest" \
        "/etc/services.d/proc-probe.toml=$root/tests/fixtures/ssh-echo/proc-probe.service.manifest"
    cargo xtask image --arch "$arch" --profile release \
        --initfs "$out/initfs.img" --output "$out/echo.img" > /dev/null
    echo "ssh-echo fixture: staged $out/ssh-echo and $out/echo.img"
fi
