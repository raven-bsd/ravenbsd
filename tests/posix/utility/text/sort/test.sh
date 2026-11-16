#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup sort

printf 'c\na\nb\n' >"$TMPDIR/in"
printf 'a\nb\nc\n' >"$TMPDIR/sorted"

text_eq "$(cat "$TMPDIR/sorted")" "$("$UTIL" "$TMPDIR/in")" "default sort"
text_eq "$(printf 'c\nb\na')" "$("$UTIL" -r "$TMPDIR/in")" "-r reverse"
text_eq "$(printf '1\n2\n10')" \
    "$(printf '10\n2\n1\n' | "$UTIL" -n)" "-n numeric"
text_eq "$(printf 'B\na')" "$(printf 'a\nB\n' | "$UTIL")" "C locale byte order"
text_eq "$(printf 'c 1\nb 2\na 3')" \
    "$(printf 'a 3\nb 2\nc 1\n' | "$UTIL" -k2,2)" "-k2,2 field key"
text_eq "$(printf 'x:1\ny:2\nz:3')" \
    "$(printf 'z:3\nx:1\ny:2\n' | "$UTIL" -t: -k2,2n)" "-t: -k2,2n"
text_eq "$(printf 'a\nb')" "$(printf 'a\na\nb\n' | "$UTIL" -u)" "-u unique"
"$UTIL" -o "$TMPDIR/out" "$TMPDIR/in"
text_file_eq "$TMPDIR/sorted" "$TMPDIR/out" "-o output file"
text_eq "$(cat "$TMPDIR/sorted")" "$("$UTIL" <"$TMPDIR/in")" "standard input"
text_status nonzero "unknown option" "$UTIL" --not-an-option
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
