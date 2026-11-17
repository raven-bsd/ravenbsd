#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup comm

printf 'a\nb\nd\n' >"$TMPDIR/a"
printf 'b\nc\nd\n' >"$TMPDIR/b"

text_eq "$(printf 'a\n\t\tb\n\tc\n\t\td')" \
    "$("$UTIL" "$TMPDIR/a" "$TMPDIR/b")" "three-column default"
text_eq "$(printf 'b\nd')" "$("$UTIL" -12 "$TMPDIR/a" "$TMPDIR/b")" "-12 common only"
text_eq "$(printf 'a\n\tc')" "$("$UTIL" -3 "$TMPDIR/a" "$TMPDIR/b")" "-3 no common column"
text_eq "$(printf '\tb\nc\n\td')" "$("$UTIL" -1 "$TMPDIR/a" "$TMPDIR/b")" "-1 suppress column one"
text_eq "$(printf 'a\n\tb\n\td')" "$("$UTIL" -2 "$TMPDIR/a" "$TMPDIR/b")" "-2 suppress column two"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/a" "$TMPDIR/missing"
