#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K495 rev185 test-only no-brush COW fixture. Build the /bin/k495-cow probe used by the
# diagnostic image; it allocates a page, forks and execs a shared-mapping child. No product
# source is changed.
#
# Usage: sh tests/fixtures/k495-cow/build.sh
# Output: build/fixtures/k495-cow/k495-cow

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k495-cow
mkdir -p "$out"
arch=x86_64
sysroot=$root/build/$arch/sysroot
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "k495-cow fixture: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "k495-cow fixture: $CC not found" >&2
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

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/fixtures/k495-cow/k495-cow.c" -o "$out/k495-cow.o"

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/k495-cow.o" \
    -L"$sysroot/usr/lib" -lc \
    "$sysroot/usr/lib/crtn.o" -o "$out/k495-cow"

# The memory-object rows need a real handle, which the C profile does not expose. Build the
# standalone Rust helper and overlay it beside the C probe in the libc-test initfs.
manifest=tests/fixtures/k495-cow/memobj/Cargo.toml
target_dir=${CARGO_TARGET_DIR:-$root/target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-k495-cow-memobj --target x86_64-unknown-none --release
cp "$target_dir/x86_64-unknown-none/release/raven-k495-cow-memobj" "$out/k495-cow-memobj"
rm -f tests/fixtures/k495-cow/memobj/Cargo.lock

echo "k495-cow fixture: staged $out/k495-cow and $out/k495-cow-memobj"
