#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup ln

printf 'hello\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a" "$TMPDIR/h"
file_file_eq "$TMPDIR/a" "$TMPDIR/h" "hard link content"
file_eq "$(stat -c '%i' "$TMPDIR/a")" "$(stat -c '%i' "$TMPDIR/h")" "hard link inode"

"$UTIL" -s "$TMPDIR/a" "$TMPDIR/s"
file_eq "$TMPDIR/a" "$(readlink "$TMPDIR/s")" "symbolic link target"
"$UTIL" -sf "$TMPDIR/a" "$TMPDIR/s"
file_eq "$TMPDIR/a" "$(readlink "$TMPDIR/s")" "force symbolic link"
file_status nonzero "missing source" "$UTIL" "$TMPDIR/missing" "$TMPDIR/x"
