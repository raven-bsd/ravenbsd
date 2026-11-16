#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup cp

printf 'hello\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a" "$TMPDIR/b"
file_file_eq "$TMPDIR/a" "$TMPDIR/b" "file copy"

mkdir "$TMPDIR/d"
"$UTIL" "$TMPDIR/a" "$TMPDIR/d"
file_file_eq "$TMPDIR/a" "$TMPDIR/d/a" "copy into directory"

chmod 640 "$TMPDIR/a"
"$UTIL" -p "$TMPDIR/a" "$TMPDIR/p"
file_eq "640" "$(stat -c '%a' "$TMPDIR/p")" "preserve mode"

mkdir -p "$TMPDIR/src/sub"
printf 'nested\n' >"$TMPDIR/src/sub/f"
"$UTIL" -r "$TMPDIR/src" "$TMPDIR/dst"
file_file_eq "$TMPDIR/src/sub/f" "$TMPDIR/dst/sub/f" "recursive copy"

file_status nonzero "missing source" "$UTIL" "$TMPDIR/missing" "$TMPDIR/out"
