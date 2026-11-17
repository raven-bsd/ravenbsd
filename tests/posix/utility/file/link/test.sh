#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup link

printf 'hello\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a" "$TMPDIR/b"
file_file_eq "$TMPDIR/a" "$TMPDIR/b" "hard link content"
file_eq "$(stat -c '%i' "$TMPDIR/a")" "$(stat -c '%i' "$TMPDIR/b")" "shared inode"
file_status nonzero "missing source" "$UTIL" "$TMPDIR/missing" "$TMPDIR/c"
