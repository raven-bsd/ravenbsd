#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sum

misc_match '^16556 +1$' "$(printf 'abc' | "$UTIL")" "default BSD checksum"
misc_match '^294 1$' "$(printf 'abc' | "$UTIL" -s)" "-s SysV checksum"
printf 'abc' >"$TMPDIR/in"
misc_match '^16556 +1 .*in$' "$("$UTIL" "$TMPDIR/in")" "file input"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
