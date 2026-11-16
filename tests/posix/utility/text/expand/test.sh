#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup expand

printf 'a\tb\n' >"$TMPDIR/in"

text_eq "$(printf 'a       b')" "$("$UTIL" "$TMPDIR/in")" "default tab stop 8"
text_eq "$(printf 'a   b')" "$("$UTIL" -t 4 "$TMPDIR/in")" "-t 4"
text_eq "$(printf 'a   b   c')" \
    "$(printf 'a\tb\tc\n' | "$UTIL" -t 4,8)" "-t 4,8 list"
text_eq "$(printf '        a b\tc')" \
    "$(printf '\ta b\tc\n' | "$UTIL" -i)" "-i initial only"
text_eq "$(printf 'a       b')" "$("$UTIL" <"$TMPDIR/in")" "standard input"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
