#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup realpath

mkdir -p "$TMPDIR/d"
printf 'target\n' >"$TMPDIR/d/a"
ln -s "$TMPDIR/d/a" "$TMPDIR/s"

file_eq "$TMPDIR/d/a" "$("$UTIL" "$TMPDIR/s")" "canonical symlink target"
file_eq "$TMPDIR" "$("$UTIL" "$TMPDIR/d/..")" "dot-dot normalised"
file_status nonzero "missing path" "$UTIL" -e "$TMPDIR/missing"
