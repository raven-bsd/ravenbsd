#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup rm

printf 'x\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a"
test ! -e "$TMPDIR/a"
file_ok "single file removed"

mkdir -p "$TMPDIR/d/sub"
printf 'y\n' >"$TMPDIR/d/sub/f"
"$UTIL" -r "$TMPDIR/d"
test ! -e "$TMPDIR/d"
file_ok "recursive directory removed"

"$UTIL" -f "$TMPDIR/missing"
file_ok "force on missing path"
file_status nonzero "missing without -f" "$UTIL" "$TMPDIR/missing"
