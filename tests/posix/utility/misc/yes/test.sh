#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup yes

printf 'y\ny\ny\n' >"$TMPDIR/want"
"$UTIL" | head -n 3 >"$TMPDIR/out"
misc_file_eq "$TMPDIR/want" "$TMPDIR/out" "default y"
printf 'hi\nhi\n' >"$TMPDIR/want2"
"$UTIL" hi | head -n 2 >"$TMPDIR/out2"
misc_file_eq "$TMPDIR/want2" "$TMPDIR/out2" "argument string"
misc_status nonzero "unknown option" "$UTIL" --not-an-option
