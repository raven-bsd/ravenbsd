#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup chown

uid=$(id -u)
gid=$(id -g)
printf 'x\n' >"$TMPDIR/f"
"$UTIL" "$uid:$gid" "$TMPDIR/f"
file_eq "$uid" "$(stat -c '%u' "$TMPDIR/f")" "numeric owner"
file_eq "$gid" "$(stat -c '%g' "$TMPDIR/f")" "numeric group"

mkdir "$TMPDIR/d"
printf 'y\n' >"$TMPDIR/d/f"
"$UTIL" -R "$uid:$gid" "$TMPDIR/d"
file_eq "$uid" "$(stat -c '%u' "$TMPDIR/d/f")" "recursive owner"
file_status nonzero "missing file" "$UTIL" "$uid:$gid" "$TMPDIR/missing"
