#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup rmdir

mkdir "$TMPDIR/empty"
"$UTIL" "$TMPDIR/empty"
test ! -e "$TMPDIR/empty"
file_ok "empty directory removed"

mkdir -p "$TMPDIR/p/q"
file_status nonzero "stop at non-empty ancestor" "$UTIL" -p "$TMPDIR/p/q"
test ! -e "$TMPDIR/p"
file_ok "parents removed with -p"

mkdir "$TMPDIR/nonempty"
printf 'x\n' >"$TMPDIR/nonempty/f"
file_status nonzero "non-empty directory" "$UTIL" "$TMPDIR/nonempty"
file_status nonzero "missing directory" "$UTIL" "$TMPDIR/missing"
