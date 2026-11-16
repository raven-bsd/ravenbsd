#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup cksum

printf 'hello\n' >"$TMPDIR/a"

file_eq "3015617425 6 $TMPDIR/a" "$("$UTIL" "$TMPDIR/a")" "known CRC and size"
file_eq "3015617425 6" "$(printf 'hello\n' | "$UTIL")" "standard input"
file_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
