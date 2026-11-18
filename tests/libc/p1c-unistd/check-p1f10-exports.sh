#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1f10 export audit. The chown-family C exports must be in Symbol.map, the generated
# Symbols.list, the frozen RAVEN_1.0 record and, when the archive has been packed,
# in build/x86_64/libc.a.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

list=libc/ravenlibc/abi/Symbols.list
frozen=libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols
map=libc/ravenlibc/crates/raven-libc/Symbol.map
archive=build/x86_64/libc.a

tmp=$(mktemp -d "${TMPDIR:-/tmp}/p1f10-exports.XXXXXX")
trap 'rm -rf "$tmp"' 0 1 2 15

names='chown fchown lchown gethostid mkfifoat mknodat'

printf '%s\n' $names >"$tmp/names"
count=$(wc -l <"$tmp/names")
[ "$count" -eq 6 ] || {
    echo "FAIL: expected 6 names, found $count" >&2
    exit 1
}

while read -r name; do
    grep -qE "(^|[[:space:]])$name;" "$map" || {
        echo "FAIL: $name is not in Symbol.map" >&2
        exit 1
    }
    grep -qx "$name" "$list" || {
        echo "FAIL: $name is not in Symbols.list" >&2
        exit 1
    }
done <"$tmp/names"

awk '{ print $1 }' "$frozen" | sort -u >"$tmp/frozen"
while read -r name; do
    grep -qx "$name" "$tmp/frozen" || {
        echo "FAIL: $name is not in the frozen RAVEN_1.0 record" >&2
        exit 1
    }
done <"$tmp/names"

if [ -f "$archive" ]; then
    nm -g --defined-only "$archive" 2>/dev/null | awk 'NF >= 3 { print $3 }' | sort -u >"$tmp/archive"
    while read -r name; do
        grep -qx "$name" "$tmp/archive" || {
            echo "FAIL: $name is not defined in $archive" >&2
            exit 1
        }
    done <"$tmp/names"
    echo "p1f10-exports: $count ABI names present in Symbol.map, Symbols.list, frozen and libc.a"
else
    echo "p1f10-exports: $count ABI names present in Symbol.map, Symbols.list and frozen (no $archive)"
fi
