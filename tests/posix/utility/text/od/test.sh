#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup od

printf 'A\n' >"$TMPDIR/in"

text_eq " 41 0a" "$("$UTIL" -An -tx1 "$TMPDIR/in")" "-An -tx1 hexadecimal"
text_eq "$(printf '   A  \\n')" "$("$UTIL" -An -c "$TMPDIR/in")" "-An -c characters"
text_eq "   A   B" "$(printf 'ABCD' | "$UTIL" -An -c -N 2)" "-N byte limit"
text_eq "   B   C   D" "$(printf 'ABCD' | "$UTIL" -An -c -j 1)" "-j skip bytes"
text_eq " 41 0a" "$("$UTIL" -An -tx1 <"$TMPDIR/in")" "standard input"
text_eq " 61 00 62 0a" "$(printf 'a\000b\n' | "$UTIL" -An -tx1)" "NUL byte display"
text_status nonzero "missing file" "$UTIL" -An -tx1 "$TMPDIR/missing"
