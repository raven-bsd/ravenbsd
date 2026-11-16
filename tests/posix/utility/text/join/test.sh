#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup join

printf '1 a\n2 b\n' >"$TMPDIR/a"
printf '1 x\n3 y\n' >"$TMPDIR/b"

text_eq "1 a x" "$("$UTIL" "$TMPDIR/a" "$TMPDIR/b")" "default join field"
text_eq "$(printf '1 a x\n2 b')" "$("$UTIL" -a1 "$TMPDIR/a" "$TMPDIR/b")" "-a1 unmatched first"
text_eq "$(printf '1 a x\n3 y')" "$("$UTIL" -a2 "$TMPDIR/a" "$TMPDIR/b")" "-a2 unmatched second"
text_eq "$(printf '1:a:x\n2:b:y')" \
    "$(printf '1:a\n2:b\n' >"$TMPDIR/ca"; printf '1:x\n2:y\n' >"$TMPDIR/cb"; "$UTIL" -t: "$TMPDIR/ca" "$TMPDIR/cb")" \
    "-t: separator"
text_eq "1 a x" "$("$UTIL" -o 0,1.2,2.2 "$TMPDIR/a" "$TMPDIR/b")" "-o output format"
text_eq "$(printf '1 a x\n2 b y')" \
    "$(printf 'a 1\nb 2\n' >"$TMPDIR/fa"; printf '1 x\n2 y\n' >"$TMPDIR/fb"; "$UTIL" -1 2 -2 1 "$TMPDIR/fa" "$TMPDIR/fb")" \
    "-1/-2 join fields"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/a" "$TMPDIR/missing"
