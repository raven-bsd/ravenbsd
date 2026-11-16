#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup nl

printf 'a\nb\n' >"$TMPDIR/in"
printf 'a\n\nb\n' >"$TMPDIR/blank"

text_eq "$(printf '     1\ta\n     2\tb')" "$("$UTIL" "$TMPDIR/in")" "default numbering"
text_eq "$(printf '1:a\n2:\n3:b')" "$("$UTIL" -w 1 -s: -b a "$TMPDIR/blank")" "-b a numbers blank lines"
text_eq "$(printf '000001\ta\n000002\tb')" "$("$UTIL" -n rz "$TMPDIR/in")" "-n rz zero padded"
text_eq "$(printf '  5:a\n  6:b')" "$("$UTIL" -w 3 -s: -v 5 "$TMPDIR/in")" "-v start number"
text_eq "$(printf '     1\ta\n     2\tb')" "$("$UTIL" <"$TMPDIR/in")" "standard input"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
