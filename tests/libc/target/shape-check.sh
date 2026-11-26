#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Interim 4 KiB ELF shape check for the C73 libc cases (PG-53). It reads the program headers
# with llvm-readelf (or readelf) and rejects an ELF that the RavenBSD loader cannot map:
# every PT_LOAD aligned to 0x1000, at most 16 PT_LOAD segments, no segment both writable and
# executable, matching file/virtual offsets modulo 0x1000, and no segment whose file range runs
# past EOF. `p_filesz` need not be page-rounded: the loader maps the file bytes and zero-fills
# the rest of the page.

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
secs=${TMPDIR:-/tmp}/rl-shape-secs-$$.txt
trap 'rm -f "$tmp" "$loads_file" "$loads_file.tls" "$secs"' EXIT HUP INT TERM

if ! "$readelf" -lW "$elf" > "$tmp" 2>/dev/null; then
    printf 'shape: FAIL cannot read program headers\n'
    exit 1
fi
if ! "$readelf" -SW "$elf" > "$secs" 2>/dev/null; then
    printf 'shape: FAIL cannot read section headers\n'
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

file_size=$(wc -c < "$elf" | tr -d ' ')
if [ -z "$file_size" ]; then
    printf 'shape: FAIL cannot size %s\n' "$elf"
    exit 1
fi

awk '$1 == "LOAD" {
        flags = ""
        for (i = 7; i < NF; i++) {
            flags = flags $i
        }
        print $2, $3, $5, flags, $NF
    }' "$tmp" > "$loads_file"

while read -r offset virt size flags align; do
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
    size_value=$((size))
    if [ $((off_value % 4096)) -ne $((virt_value % 4096)) ]; then
        printf 'shape: FAIL offset/vaddr mismatch at %s\n' "$virt"
        exit 1
    fi
    if [ "$size_value" -ne 0 ] && [ $((off_value + size_value)) -gt "$file_size" ]; then
        printf 'shape: FAIL segment past EOF at %s (offset %s + size %s > %s)\n' \
            "$virt" "$offset" "$size" "$file_size"
        exit 1
    fi
done < "$loads_file"

# The static TLS image is an ELF invariant the loader and libc both consume: a section named
# `.tdata`/`.tbss` without a PT_TLS is a foreign TLS image that __raven_init_main_thread cannot
# find, and a PT_TLS that does not cover its sections leaves the thread pointer over the wrong
# memory.
awk '
function hex(value, alphabet, result, i) {
    alphabet = "0123456789abcdef"
    sub(/^0x/, "", value)
    result = 0
    for (i = 1; i <= length(value); i++) {
        result = result * 16 + index(alphabet, tolower(substr(value, i, 1))) - 1
    }
    return result
}
{
    line = $0
    if (line ~ /^ *\[ *[0-9]+\] /) {
        sub(/^ *\[ *[0-9]+\] +/, "", line)
        split(line, field, / +/)
        if (field[1] == ".tdata" || field[1] == ".tbss") {
            print field[1], field[2], hex(field[3]), hex(field[5])
        }
    }
}' "$secs" > "$loads_file.tls"

tls_count=$(awk '$1 == "TLS" { count = count + 1 } END { print count + 0 }' "$tmp")
if [ "$tls_count" -gt 1 ]; then
    printf 'shape: FAIL %s PT_TLS segments\n' "$tls_count"
    exit 1
fi
if [ "$tls_count" -eq 0 ]; then
    if grep -qE '^\.(tdata|tbss) ' "$loads_file.tls"; then
        printf 'shape: FAIL TLS sections without PT_TLS\n'
        exit 1
    fi
else
    tls_line=$(awk '$1 == "TLS" { print $2, $3, $5, $6, $NF; exit }' "$tmp")
    set -- $tls_line
    tls_vaddr=$2
    tls_filesz=$3
    tls_memsz=$4
    tls_align=$5
    if [ $((tls_align)) -le 0 ] || [ $((tls_align & (tls_align - 1))) -ne 0 ]; then
        printf 'shape: FAIL PT_TLS align %s (want a power of two)\n' "$tls_align"
        exit 1
    fi
    if [ $((tls_vaddr % tls_align)) -ne 0 ]; then
        printf 'shape: FAIL PT_TLS vaddr %s not aligned to %s\n' "$tls_vaddr" "$tls_align"
        exit 1
    fi
    if [ "$((tls_filesz))" -gt "$((tls_memsz))" ]; then
        printf 'shape: FAIL PT_TLS filesz %s exceeds memsz %s\n' "$tls_filesz" "$tls_memsz"
        exit 1
    fi
    while read -r sec_name sec_type sec_addr sec_size; do
        case $sec_name in
            .tdata)
                if [ "$sec_type" != "PROGBITS" ]; then
                    printf 'shape: FAIL .tdata type %s (want PROGBITS)\n' "$sec_type"
                    exit 1
                fi
                if [ "$((sec_addr))" -lt "$((tls_vaddr))" ] ||
                    [ "$(((sec_addr - tls_vaddr) + sec_size))" -gt "$((tls_filesz))" ]; then
                    printf 'shape: FAIL .tdata outside the PT_TLS file image\n'
                    exit 1
                fi
                ;;
            .tbss)
                if [ "$sec_type" != "NOBITS" ]; then
                    printf 'shape: FAIL .tbss type %s (want NOBITS)\n' "$sec_type"
                    exit 1
                fi
                if [ "$((sec_addr))" -lt "$((tls_vaddr))" ] ||
                    [ "$(((sec_addr - tls_vaddr) + sec_size))" -gt "$((tls_memsz))" ]; then
                    printf 'shape: FAIL .tbss outside the PT_TLS memory image\n'
                    exit 1
                fi
                ;;
        esac
    done < "$loads_file.tls"
fi

printf 'shape: ok\n'
