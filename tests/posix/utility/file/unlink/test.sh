#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup unlink

printf 'x\n' >"$TMPDIR/a"
"$UTIL" "$TMPDIR/a"
test ! -e "$TMPDIR/a"
file_ok "file removed"

mkdir "$TMPDIR/d"
file_status nonzero "directory refused" "$UTIL" "$TMPDIR/d"
file_status nonzero "missing path" "$UTIL" "$TMPDIR/missing"
