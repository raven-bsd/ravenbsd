#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup df

out=$("$UTIL" -P "$TMPDIR")
file_contains "$out" "Filesystem" "POSIX header"
last=$(printf '%s\n' "$out" | tail -n 1)
file_contains "$last" "%" "capacity field"
file_status nonzero "missing path" "$UTIL" -P "$TMPDIR/missing"
