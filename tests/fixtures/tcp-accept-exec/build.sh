#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K225 accepted-socket exec fixture: the driver and its static writer
# are packed over the production `server` image used by the `hw-net` profile.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/tcp-accept-exec
mkdir -p "$out"

# Populate the libc sysroot and the loader/libc files the server image expects.
sh tests/fixtures/socketpair-exec/build.sh >/dev/null

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "tcp-accept-exec fixture: $CC not found" >&2
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

sysroot=$root/build/x86_64/sysroot
build_binary() {
    source=$1
    object=$2
    binary=$3

    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
        -c "$source" -o "$object"
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$object" \
        -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" \
        -o "$binary"
}

build_binary "$root/tests/fixtures/tcp-accept-exec/main.c" "$out/main.o" "$out/main"
build_binary "$root/tests/fixtures/tcp-accept-exec/writer.c" "$out/writer.o" "$out/writer"

# The hw-net profile stages build/fixtures/b70-net/probe; build a valid stand-in
# and then replace it with the K225 driver in the overlay.
target_dir=${CARGO_TARGET_DIR:-$root/target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-b70-probe --release --target x86_64-unknown-none >/dev/null
mkdir -p build/fixtures/b70-net
cp "$target_dir/x86_64-unknown-none/release/raven-b70-probe" build/fixtures/b70-net/probe

RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile hw-net >/dev/null
cp build/x86_64/initfs.img "$out/base.img"

cargo build -p raven-b70-overlay >/dev/null
"$target_dir/debug/raven-b70-overlay" \
    "$out/base.img" \
    "$out/initfs.img" \
    "/sbin/b70-net-probe=$out/main:0755" \
    "/bin/tcp-accept-writer=$out/writer:0755"

echo "tcp-accept-exec fixture: $out/initfs.img"
