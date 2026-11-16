#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup readlink

printf 'target\n' >"$TMPDIR/a"
ln -s "$TMPDIR/a" "$TMPDIR/s"

file_eq "$TMPDIR/a" "$("$UTIL" "$TMPDIR/s")" "symlink target"
file_eq "$TMPDIR/a" "$("$UTIL" -f "$TMPDIR/s")" "canonical target"
file_status nonzero "missing path" "$UTIL" "$TMPDIR/missing"
