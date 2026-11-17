#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b stdio batch-B evidence: the stdio engine tests and the new symbols.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-stdio

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix stdio2: $archive is missing" >&2; exit 1; }

for symbol in \
    cuserid dprintf gets renameat2 tmpfile tmpfile64 tmpnam vdprintf vsnprintf_l
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix stdio2: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix stdio2: PASS"
