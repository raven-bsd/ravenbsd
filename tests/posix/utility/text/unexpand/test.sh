#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup unexpand

printf '        a\n' >"$TMPDIR/leading"
printf 'a       b\n' >"$TMPDIR/middle"

text_eq "$(printf '\ta')" "$("$UTIL" "$TMPDIR/leading")" "default leading blanks"
text_eq "$(printf 'a       b')" "$("$UTIL" "$TMPDIR/middle")" "default leaves middle blanks"
text_eq "$(printf 'a\tb')" "$("$UTIL" -a "$TMPDIR/middle")" "-a all blanks"
text_eq "$(printf '\ta')" "$(printf '    a\n' | "$UTIL" -t 4)" "-t 4"
text_eq "$(printf '\ta')" "$("$UTIL" <"$TMPDIR/leading")" "standard input"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
