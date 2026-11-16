#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup pr

printf 'a\nb\n' >"$TMPDIR/in"

text_eq "$(printf 'a\nb')" "$("$UTIL" -t "$TMPDIR/in")" "-t omit header and trailer"
text_eq "$(printf '    1\ta\n    2\tb')" "$("$UTIL" -t -n "$TMPDIR/in")" "-n number lines"
text_eq "$(printf 'a\nb')" "$("$UTIL" -t <"$TMPDIR/in")" "standard input"
text_status nonzero "missing file" "$UTIL" -t "$TMPDIR/missing"
