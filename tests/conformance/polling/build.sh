#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K548 polling guest client into build/libc-test/libc-test. The
# libc-test profile stages it as /bin/libc-test and runs it as the one-shot
# service, so the pty server is available for the write-only pty-slave case.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
if [ "$arch" != x86_64 ]; then
    printf 'polling: skipped: arch %s is not this fixture\n' "$arch"
    exit 77
fi

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" polling

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'polling: skipped: %s not found\n' "$CC" >&2
    exit 77
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
    printf 'polling: skipped: ld.lld not found\n' >&2
    exit 77
fi

out=$root/build/libc-test
mkdir -p "$out/obj"

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/conformance/polling/main.c" -o "$out/obj/polling.o"

if [ -n "$builtins" ]; then
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/polling.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
else
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/obj/polling.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/libc-test"
fi

# The libc-test profile overlay stages these five artifacts even for a static case.
: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"

printf 'polling\n' > "$out/stamp"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
printf 'polling: staged %s\n' "$out/libc-test"
