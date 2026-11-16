#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup chgrp

gid=$(id -g)
printf 'x\n' >"$TMPDIR/f"
"$UTIL" "$gid" "$TMPDIR/f"
file_eq "$gid" "$(stat -c '%g' "$TMPDIR/f")" "numeric group"

mkdir "$TMPDIR/d"
printf 'y\n' >"$TMPDIR/d/f"
"$UTIL" -R "$gid" "$TMPDIR/d"
file_eq "$gid" "$(stat -c '%g' "$TMPDIR/d/f")" "recursive group"
file_status nonzero "missing file" "$UTIL" "$gid" "$TMPDIR/missing"
