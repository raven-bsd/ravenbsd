#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup mknod

"$UTIL" "$TMPDIR/f" p
test -p "$TMPDIR/f"
file_ok "FIFO node created"

"$UTIL" -m 600 "$TMPDIR/g" p
test -p "$TMPDIR/g"
file_eq "600" "$(stat -c '%a' "$TMPDIR/g")" "explicit mode"
file_status nonzero "missing operand" "$UTIL"
