#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b wchar/wctype behavior evidence: the UTF-8 classification/codec suite plus a defined-symbol
# check for every claimed row.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-ctype

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix wchar: $archive is missing" >&2; exit 1; }

for symbol in \
    iswalnum_l iswalpha_l iswblank_l iswcntrl_l iswctype_l iswdigit_l iswgraph_l \
    iswlower_l iswprint_l iswpunct_l iswspace_l iswupper_l iswxdigit_l towctrans_l \
    towlower_l towupper_l wctrans_l wctype_l \
    wcscasecmp_l wcscoll wcscoll_l wcsncasecmp_l wcsnlen wcstok wcsxfrm wcsxfrm_l wcsftime_l
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix wchar: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix wchar: PASS"
