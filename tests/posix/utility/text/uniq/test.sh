#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup uniq

printf 'a\na\nb\na\n' >"$TMPDIR/in"
printf 'a\na\nb\n' >"$TMPDIR/dup"

text_eq "$(printf 'a\nb\na')" "$("$UTIL" "$TMPDIR/in")" "adjacent duplicate removal"
text_eq "$(printf '      2 a\n      1 b')" "$("$UTIL" -c "$TMPDIR/dup")" "-c count"
text_eq "a" "$("$UTIL" -d "$TMPDIR/dup")" "-d repeated only"
text_eq "b" "$("$UTIL" -u "$TMPDIR/dup")" "-u unique only"
text_eq "$(printf 'x a\ny b')" \
    "$(printf 'x a\nx a\ny b\n' | "$UTIL" -f 1)" "-f skip fields"
"$UTIL" "$TMPDIR/in" "$TMPDIR/out"
printf 'a\nb\na\n' >"$TMPDIR/want"
text_file_eq "$TMPDIR/want" "$TMPDIR/out" "input and output operands"
text_status nonzero "unknown option" "$UTIL" --not-an-option
