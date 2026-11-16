#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup ptx

printf 'one two\nthree four\n' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/out"
misc_match 'one' "$(cat "$TMPDIR/out")" "simple index"
misc_match 'three' "$(cat "$TMPDIR/out")" "simple index second word"
"$UTIL" -G "$TMPDIR/in" >"$TMPDIR/roff"
misc_match '^\.xx ' "$(cat "$TMPDIR/roff")" "-G roff output"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
