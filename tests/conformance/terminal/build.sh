#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S742 test-only: build the Rust /bin/terminal-conformance probe against ravenlibc for the selected
# architecture. The old terminal-conformance.c blob stays byte-identical (the structural gate
# forbids C changes); the probe and its expected FreeBSD record are Rust-owned now.
#
# Usage: sh tests/conformance/terminal/build.sh [x86_64|aarch64]
# Output: build/conformance/terminal/terminal-conformance

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        rust_target=x86_64-unknown-none
        ;;
    aarch64)
        rust_target=aarch64-unknown-none-softfloat
        ;;
    *)
        printf 'terminal-conformance: unsupported arch %s\n' "$arch" >&2
        exit 2
        ;;
esac

out=$root/build/conformance/terminal
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot
linker=$root/userland/linker/$arch.ld

# Refresh the static sysroot from the current libc sources (the same step the libc cases use).
sh tests/libc/target/build.sh termios "$arch" >/dev/null 2>&1 || true

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "terminal-conformance: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done
[ -f "$linker" ] || {
    echo "terminal-conformance: missing linker script $linker" >&2
    exit 1
}

rustc --target "$rust_target" --edition 2024 -O -C panic=abort \
    -C relocation-model=static -C code-model=small \
    -C link-arg=-static -C link-arg=-T"$linker" \
    -C link-arg=-zmax-page-size=4096 -C link-arg=-zcommon-page-size=4096 \
    -C link-arg="$sysroot/usr/lib/crt1.o" \
    -C link-arg="$sysroot/usr/lib/crti.o" \
    -C link-arg="$sysroot/usr/lib/libc.a" \
    -C link-arg="$sysroot/usr/lib/crtn.o" \
    "$root/tests/conformance/terminal/terminal-conformance.rs" \
    -o "$out/terminal-conformance"

sh "$root/tests/libc/target/shape-check.sh" "$out/terminal-conformance"
echo "terminal-conformance: staged $out/terminal-conformance"
