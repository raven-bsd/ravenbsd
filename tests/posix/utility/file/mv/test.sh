#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup mv

printf 'hello\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a" "$TMPDIR/b"
test ! -e "$TMPDIR/a"
file_ok "source removed"
file_eq "hello" "$(cat "$TMPDIR/b")" "renamed content"

mkdir "$TMPDIR/d"
printf 'world\n' >"$TMPDIR/c"
"$UTIL" "$TMPDIR/c" "$TMPDIR/d"
file_eq "world" "$(cat "$TMPDIR/d/c")" "move into directory"

printf 'force\n' >"$TMPDIR/f"
"$UTIL" -f "$TMPDIR/f" "$TMPDIR/d/f"
file_eq "force" "$(cat "$TMPDIR/d/f")" "force overwrite"
file_status nonzero "missing source" "$UTIL" "$TMPDIR/missing" "$TMPDIR/out"
