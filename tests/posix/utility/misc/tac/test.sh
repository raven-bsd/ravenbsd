#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup tac

printf 'a\nb\nc\n' >"$TMPDIR/in"
misc_eq "$(printf 'c\nb\na')" "$("$UTIL" "$TMPDIR/in")" "reverse lines"
printf 'a,b,c' >"$TMPDIR/comma"
misc_eq "cb,a," "$("$UTIL" -s, "$TMPDIR/comma")" "-s separator"
misc_eq "$(printf 'c\nb\na')" "$("$UTIL" <"$TMPDIR/in")" "standard input"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
