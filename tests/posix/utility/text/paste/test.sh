#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup paste

printf 'a\nb\n' >"$TMPDIR/a"
printf '1\n2\n' >"$TMPDIR/b"

text_eq "$(printf 'a\t1\nb\t2')" "$("$UTIL" "$TMPDIR/a" "$TMPDIR/b")" "tab-separated columns"
text_eq "$(printf 'a,1\nb,2')" "$("$UTIL" -d, "$TMPDIR/a" "$TMPDIR/b")" "-d, delimiter"
text_eq "$(printf 'a\tb\n1\t2')" "$("$UTIL" -s "$TMPDIR/a" "$TMPDIR/b")" "-s serial"
text_eq "$(printf 'a,b\n1,2')" "$("$UTIL" -s -d, "$TMPDIR/a" "$TMPDIR/b")" "-s -d,"
text_eq "$(printf 'x\t1\ny\t2')" \
    "$(printf 'x\ny\n' | "$UTIL" - "$TMPDIR/b")" "standard input operand"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/a" "$TMPDIR/missing"
