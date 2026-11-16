#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup stat

printf 'hello\n' >"$TMPDIR/a"
chmod 640 "$TMPDIR/a"

file_eq "$TMPDIR/a 6" "$("$UTIL" -c '%n %s' "$TMPDIR/a")" "name and size"
file_eq "640" "$("$UTIL" -c '%a' "$TMPDIR/a")" "mode field"
file_status nonzero "missing path" "$UTIL" -c '%n' "$TMPDIR/missing"
