#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b string.h behavior evidence: run the collation engine tests and verify the exported
# strcoll/strxfrm symbols are present in the packaged libc.a.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-locale -E 'test(collate)'

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix string: $archive is missing" >&2; exit 1; }

for symbol in strcoll strcoll_l strxfrm strxfrm_l strerror_l; do
    if ! nm -g --defined-only "$archive" | grep -Eq " $symbol$"; then
        echo "posix string: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix string: PASS"
