#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup fold

printf 'abcdefghij\n' >"$TMPDIR/in"
printf 'ab cd ef\n' >"$TMPDIR/spaces"

text_eq "$(printf 'abcde\nfghij')" "$("$UTIL" -w 5 "$TMPDIR/in")" "-w 5"
text_eq "$(printf 'abcde\nfghij')" "$("$UTIL" -w 5 -b "$TMPDIR/in")" "-b byte columns"
text_eq "$(printf 'ab \ncd ef')" "$("$UTIL" -s -w 5 "$TMPDIR/spaces")" "-s break at blanks"
text_eq "$(printf 'abcde\nfghij')" "$("$UTIL" -w 5 <"$TMPDIR/in")" "standard input"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
