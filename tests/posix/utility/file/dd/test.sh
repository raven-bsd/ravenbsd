#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup dd

printf 'abcdef' >"$TMPDIR/in"
"$UTIL" if="$TMPDIR/in" of="$TMPDIR/out" bs=1 count=3 2>"$TMPDIR/dd.err"
file_eq "abc" "$(cat "$TMPDIR/out")" "bs/count copy"

"$UTIL" if="$TMPDIR/in" of="$TMPDIR/skip" bs=1 skip=1 count=2 2>"$TMPDIR/dd.err"
file_eq "bc" "$(cat "$TMPDIR/skip")" "skip and count"

file_status nonzero "missing input" "$UTIL" if="$TMPDIR/missing" of="$TMPDIR/x"
