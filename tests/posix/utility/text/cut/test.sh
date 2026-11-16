#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup cut

printf 'a:b:c\nx:y:z\n' >"$TMPDIR/in"

text_eq "$(printf 'b\ny')" "$("$UTIL" -f2 -d: "$TMPDIR/in")" "-f2 -d:"
text_eq "bcd" "$(printf 'abcdef\n' | "$UTIL" -c2-4)" "-c2-4 character list"
text_eq "ac" "$(printf 'abcdef\n' | "$UTIL" -b1,3)" "-b1,3 byte list"
text_eq "b" "$(printf 'a:b\nnodelim\n' | "$UTIL" -s -f2 -d:)" "-s suppress"
text_eq "$(printf 'b\nnodelim')" \
    "$(printf 'a:b\nnodelim\n' | "$UTIL" -f2 -d:)" "no delimiter passes through"
text_eq "$(printf 'b\ny')" "$("$UTIL" -f2 -d: - <"$TMPDIR/in")" "stdin operand"
printf 'a\000b\n' >"$TMPDIR/nul"
printf 'a\000b\n' >"$TMPDIR/want-nul"
"$UTIL" -b1-3 "$TMPDIR/nul" >"$TMPDIR/out-nul"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/out-nul" "NUL byte in byte list"
text_status nonzero "unknown option" "$UTIL" --not-an-option
text_status nonzero "multi-character delimiter" \
    "$UTIL" -d:: -f2 "$TMPDIR/in"
