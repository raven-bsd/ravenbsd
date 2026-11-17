#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b stdlib.h behavior evidence: run the float and multibyte conversion engine tests and verify
# the exported atof/strtod/mbstowcs/wcstombs symbols are present in the packaged libc.a.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-dtoa -E 'test(strtod)'
cargo nextest run -p raven-libc-ctype -E 'test(mbstowcs) | test(wcstombs)'
cargo nextest run -p raven-libc-malloc -E 'test(free_sized) | test(free_aligned_sized)'

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix stdlib: $archive is missing" >&2; exit 1; }

for symbol in atof strtod mbstowcs wcstombs free_sized free_aligned_sized; do
    if ! nm -g --defined-only "$archive" | grep -Eq " $symbol$"; then
        echo "posix stdlib: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix stdlib: PASS"
