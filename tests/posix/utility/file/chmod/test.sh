#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup chmod

printf 'x\n' >"$TMPDIR/f"
"$UTIL" 0640 "$TMPDIR/f"
file_eq "640" "$(stat -c '%a' "$TMPDIR/f")" "numeric mode"
"$UTIL" u+x "$TMPDIR/f"
file_eq "740" "$(stat -c '%a' "$TMPDIR/f")" "symbolic mode"

mkdir "$TMPDIR/d"
printf 'y\n' >"$TMPDIR/d/f"
"$UTIL" -R 0750 "$TMPDIR/d"
file_eq "750" "$(stat -c '%a' "$TMPDIR/d")" "recursive directory"
file_eq "750" "$(stat -c '%a' "$TMPDIR/d/f")" "recursive file"
file_status nonzero "missing file" "$UTIL" 0644 "$TMPDIR/missing"
