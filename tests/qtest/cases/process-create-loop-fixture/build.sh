#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S192 100-spawn initfs. The parent posix_spawns the child 100 times; no services.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=x86_64
sysroot=$root/build/$arch/sysroot
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "process-create-loop: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done
CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || { echo "process-create-loop: $CC not found" >&2; exit 77; }
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
    [ -x "$lld_dir/ld.lld" ] && PATH="$lld_dir:$PATH" && export PATH
fi

out=build/qtest/process-create-loop
mkdir -p "$out"
compile() {
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -D__BSD_VISIBLE=1 -mno-sse -mno-mmx -msoft-float \
        -c "$1" -o "$2"
}
link() {
    "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$1" \
        -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" -o "$2"
}
compile tests/qtest/cases/process-create-loop-fixture/parent.c "$out/parent.o"
link "$out/parent.o" "$out/parent"
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$out/parent:0755"
echo "process-create-loop: staged $out/initfs.img"
