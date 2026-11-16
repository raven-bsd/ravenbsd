#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup mkdir

"$UTIL" "$TMPDIR/d"
test -d "$TMPDIR/d"
file_ok "single directory"

"$UTIL" -p "$TMPDIR/p/q/r"
test -d "$TMPDIR/p/q/r"
file_ok "parents with -p"

"$UTIL" -m 750 "$TMPDIR/md"
file_eq "750" "$(stat -c '%a' "$TMPDIR/md")" "explicit mode"
file_status nonzero "existing directory" "$UTIL" "$TMPDIR/d"
file_status nonzero "missing operand" "$UTIL"
