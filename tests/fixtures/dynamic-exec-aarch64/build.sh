#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S82 aarch64 dynamic-exec fixture. The aarch64 sysroot must already contain
# Scrt1.o, libc.so.1 and ld-elf.so.1 (cargo xtask libc-pack/libc-link --arch aarch64); this
# script does not build a missing sysroot because the aarch64 libc build is a separate lane.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-aarch64}}
if [ "$arch" != aarch64 ]; then
    printf 'skipped: dynamic-exec-aarch64 is aarch64 only\n'
    exit 77
fi

out=build/fixtures/dynamic-exec-aarch64
mkdir -p "$out"

sysroot=${RAVENLIBC_SYSROOT:-$root/build/aarch64/sysroot}
for file in \
    "$sysroot/usr/lib/Scrt1.o" \
    "$sysroot/usr/lib/crti.o" \
    "$sysroot/usr/lib/crtn.o" \
    "$sysroot/lib/libc.so.1" \
    "$sysroot/libexec/ld-elf.so.1"; do
    if [ ! -f "$file" ]; then
        printf 'dynamic-exec-aarch64: missing %s (build the aarch64 sysroot first)\n' "$file" >&2
        exit 1
    fi
done

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'dynamic-exec-aarch64: %s not found\n' "$CC" >&2
    exit 1
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    printf 'dynamic-exec-aarch64: ld.lld not found\n' >&2
    exit 1
fi

readelf=${READELF:-}
if [ -z "$readelf" ]; then
    if command -v llvm-readelf >/dev/null 2>&1; then
        readelf=llvm-readelf
    else
        readelf=readelf
    fi
fi

"$CC" --target=aarch64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -fPIE -D__ravenbsd__=1 -D__unix__=1 \
    -c tests/fixtures/dynamic-exec-aarch64/dynamic-exec.c -o "$out/dynamic-exec.o"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi
if [ -n "$builtins" ]; then
    "$CC" --target=aarch64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld -Wl,-m,aarch64linux \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--allow-shlib-undefined \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/dynamic-exec.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/dynamic-exec"
else
    "$CC" --target=aarch64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld -Wl,-m,aarch64linux \
        -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
        -Wl,--allow-shlib-undefined \
        -Wl,--image-base=0x400000 -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$out/dynamic-exec.o" \
        -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/dynamic-exec"
fi

if ! "$readelf" -h "$out/dynamic-exec" | grep -q 'Type:.*DYN'; then
    printf 'dynamic-exec-aarch64: target is not ET_DYN\n' >&2
    exit 1
fi
if ! "$readelf" -l "$out/dynamic-exec" |
    grep -q 'Requesting program interpreter: /libexec/ld-elf.so.1'; then
    printf 'dynamic-exec-aarch64: target has no PT_INTERP ld-elf.so.1\n' >&2
    exit 1
fi
if ! "$readelf" -d "$out/dynamic-exec" | grep -q 'NEEDED.*libc.so.1'; then
    printf 'dynamic-exec-aarch64: target does not need libc.so.1\n' >&2
    exit 1
fi

# The libc-test profile packs these paths; substitute the aarch64 images and the launcher.
profile_files=$root/build/libc-test
mkdir -p "$profile_files"
cp "$sysroot/libexec/ld-elf.so.1" "$profile_files/ld-elf.so.1"
cp "$sysroot/lib/libc.so.1" "$profile_files/libc.so.1"
: > "$profile_files/libc-child"
: > "$profile_files/libplug.so"

target_dir=${CARGO_TARGET_DIR:-$root/target}
DYNAMIC_EXEC_IMAGE="$root/$out/dynamic-exec" \
    CARGO_TARGET_DIR="$target_dir" \
    RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld" \
    cargo build --manifest-path "$root/tests/fixtures/dynamic-exec-aarch64/Cargo.toml" \
    -p raven-dynamic-exec-aarch64 --target aarch64-unknown-none-softfloat --release
target=$target_dir/aarch64-unknown-none-softfloat/release
cp "$target/raven-dynamic-exec-aarch64-init" "$profile_files/libc-test"
