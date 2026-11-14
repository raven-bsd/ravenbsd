#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Interim 4 KiB ELF shape check for the C73 libc cases (PG-53). It reads the program headers
# with llvm-readelf (or readelf) and rejects an ELF that the RavenBSD loader cannot map:
# every PT_LOAD aligned to 0x1000, at most 16 PT_LOAD segments, no segment both writable and
# executable, and matching file/virtual offsets modulo 0x1000.

set -eu

if [ "$#" -ne 1 ]; then
    printf 'usage: %s <elf>\n' "$0" >&2
    exit 2
fi

elf=$1
if [ ! -f "$elf" ]; then
    printf 'shape: FAIL missing %s\n' "$elf"
    exit 1
fi

if [ -n "${READELF:-}" ]; then
    readelf=$READELF
elif command -v llvm-readelf >/dev/null 2>&1; then
    readelf=llvm-readelf
elif command -v readelf >/dev/null 2>&1; then
    readelf=readelf
else
    printf 'shape: FAIL no llvm-readelf or readelf\n'
    exit 1
fi

tmp=${TMPDIR:-/tmp}/rl-shape-$$.txt
loads_file=${TMPDIR:-/tmp}/rl-shape-loads-$$.txt
trap 'rm -f "$tmp" "$loads_file"' EXIT HUP INT TERM

if ! "$readelf" -lW "$elf" > "$tmp" 2>/dev/null; then
    printf 'shape: FAIL cannot read program headers\n'
    exit 1
fi

loads=$(awk '$1 == "LOAD" { count = count + 1 } END { print count + 0 }' "$tmp")
if [ "$loads" -eq 0 ]; then
    printf 'shape: FAIL no PT_LOAD segments\n'
    exit 1
fi
if [ "$loads" -gt 16 ]; then
    printf 'shape: FAIL %s PT_LOAD segments (limit 16)\n' "$loads"
    exit 1
fi

awk '$1 == "LOAD" {
        flags = ""
        for (i = 7; i < NF; i++) {
            flags = flags $i
        }
        print $2, $3, flags, $NF
    }' "$tmp" > "$loads_file"

while read -r offset virt flags align; do
    case $flags in
        *W*E* | *E*W*)
            printf 'shape: FAIL writable and executable PT_LOAD at %s\n' "$virt"
            exit 1
            ;;
    esac
    if [ "$align" != 0x1000 ] && [ "$align" != 0x00001000 ]; then
        printf 'shape: FAIL PT_LOAD align %s at %s (want 0x1000)\n' "$align" "$virt"
        exit 1
    fi
    off_value=$((offset))
    virt_value=$((virt))
    if [ $((off_value % 4096)) -ne $((virt_value % 4096)) ]; then
        printf 'shape: FAIL offset/vaddr mismatch at %s\n' "$virt"
        exit 1
    fi
done < "$loads_file"

printf 'shape: ok\n'
