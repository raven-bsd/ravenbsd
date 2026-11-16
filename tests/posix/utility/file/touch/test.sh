#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup touch

"$UTIL" "$TMPDIR/a"
test -f "$TMPDIR/a"
file_ok "file created"

"$UTIL" -c "$TMPDIR/b"
test ! -e "$TMPDIR/b"
file_ok "no-create leaves the path absent"

expected=$(date -d '2020-01-02 03:04:05' +%s)
"$UTIL" -t 202001020304.05 "$TMPDIR/a"
file_eq "$expected" "$(stat -c '%Y' "$TMPDIR/a")" "timestamp with -t"

printf 'ref\n' >"$TMPDIR/ref"
"$UTIL" -r "$TMPDIR/ref" "$TMPDIR/c"
file_eq "$(stat -c '%Y' "$TMPDIR/ref")" "$(stat -c '%Y' "$TMPDIR/c")" "reference timestamp"
file_status nonzero "missing parent" "$UTIL" "$TMPDIR/no/such/path"
