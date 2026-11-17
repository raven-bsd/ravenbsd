#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b stdlib batch-B evidence: the quick-exit registry tests and the new symbols.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-stdlib

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix stdlib2: $archive is missing" >&2; exit 1; }

for symbol in at_quick_exit ecvt_r fcvt_r quick_exit mkstemp strtold
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix stdlib2: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix stdlib2: PASS"
