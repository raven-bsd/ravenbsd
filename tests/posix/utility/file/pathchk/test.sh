#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup pathchk

mkdir -p "$TMPDIR/d"
printf 'x\n' >"$TMPDIR/d/f"

file_status 0 "portable path" "$UTIL" -p d/f
file_status 0 "relative path" "$UTIL" d/f
file_status nonzero "empty path" "$UTIL" ""
