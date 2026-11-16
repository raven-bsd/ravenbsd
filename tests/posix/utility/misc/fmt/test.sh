#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup fmt

printf 'one two three four five six\n' >"$TMPDIR/in"
misc_eq "$(printf 'one two\nthree four\nfive six')" \
    "$("$UTIL" -w 10 "$TMPDIR/in")" "-w 10 wrap"
misc_eq "$(printf 'one two\nthree four\nfive six')" \
    "$("$UTIL" <"$TMPDIR/in" | "$UTIL" -w 10)" "standard input"
misc_status nonzero "unknown option" "$UTIL" --not-an-option
