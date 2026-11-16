#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup wc

printf 'one two\nthree four five\n' >"$TMPDIR/in"

text_eq "2 $TMPDIR/in" "$("$UTIL" -l "$TMPDIR/in")" "-l lines"
text_eq "5 $TMPDIR/in" "$("$UTIL" -w "$TMPDIR/in")" "-w words"
text_eq "24 $TMPDIR/in" "$("$UTIL" -c "$TMPDIR/in")" "-c bytes"
text_eq "24 $TMPDIR/in" "$("$UTIL" -m "$TMPDIR/in")" "-m characters"
text_eq "      1       2       4" "$(printf 'a b\n' | "$UTIL")" "standard input"
text_eq "4 10 48 total" \
    "$("$UTIL" -l -w -c "$TMPDIR/in" "$TMPDIR/in" | tail -n 1 | sed 's/^ *//')" \
    "multiple files total"
printf 'a\000b' >"$TMPDIR/nul"
text_eq "3 $TMPDIR/nul" "$("$UTIL" -c "$TMPDIR/nul")" "-c counts NUL bytes"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
