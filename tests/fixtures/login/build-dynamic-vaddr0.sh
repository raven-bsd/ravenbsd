#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build a release initfs whose dynamic interpreter has the release-image shape: the first
# PT_LOAD starts at vaddr=0. The pre-S129 raven-spawn parser rejects that shape with
# SegmentOutsideUserRange; the S129 parser accepts it and applies the non-zero interpreter bias.
# The staged `build/<arch>/ld-elf.so.1` is restored before this script exits, so the artifact is
# not left mutated for later cases.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
loader="build/$arch/ld-elf.so.1"
out="build/fixtures/login/release-dynamic-vaddr0.img"

if [ ! -f "$loader" ]; then
    printf 'login fixture: missing %s; run cargo xtask libc-link --arch %s first\n' "$loader" "$arch" >&2
    exit 2
fi

mkdir -p "$(dirname -- "$out")"
backup=$(mktemp "${TMPDIR:-/tmp}/ld-elf.XXXXXX")
cp "$loader" "$backup"
restore() {
    cp "$backup" "$loader"
    rm -f "$backup"
}
trap restore EXIT INT TERM HUP

phoff=$(readelf -h "$loader" | awk '/Start of program headers:/ {print $5}')
phentsize=$(readelf -h "$loader" | awk '/Size of program headers:/ {print $5}')
phnum=$(readelf -h "$loader" | awk '/Number of program headers:/ {print $5}')

i=0
patched=0
while [ "$i" -lt "$phnum" ]; do
    off=$((phoff + i * phentsize))
    type=$(od -An -tu4 -j "$off" -N4 "$loader" | tr -d ' ')
    if [ "$type" = "1" ]; then
        # ELF64 phdr: p_vaddr at +16, p_paddr at +24.
        dd if=/dev/zero of="$loader" bs=1 seek=$((off + 16)) count=8 conv=notrunc 2>/dev/null
        dd if=/dev/zero of="$loader" bs=1 seek=$((off + 24)) count=8 conv=notrunc 2>/dev/null
        patched=1
        break
    fi
    i=$((i + 1))
done

if [ "$patched" != "1" ]; then
    printf 'login fixture: no PT_LOAD in %s\n' "$loader" >&2
    exit 3
fi

first_load=$(readelf -lW "$loader" | awk '/^  LOAD / {print; exit}')
case "$first_load" in
    *0x0000000000000000*) ;;
    *)
        printf 'login fixture: vaddr-0 patch failed: %s\n' "$first_load" >&2
        exit 4
        ;;
esac

cargo xtask initfs --arch "$arch" --profile release
cp "build/$arch/initfs.img" "$out"
