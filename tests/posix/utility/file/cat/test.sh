#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup cat

printf 'hello\n' >"$TMPDIR/a"
printf 'world\n' >"$TMPDIR/b"

file_eq "hello" "$("$UTIL" "$TMPDIR/a")" "single file"
file_eq "hello" "$(printf 'hello\n' | "$UTIL")" "standard input"
file_eq "hello
world" "$("$UTIL" "$TMPDIR/a" "$TMPDIR/b")" "multiple files"
file_eq "hello" "$(printf 'hello\n' | "$UTIL" -)" "dash standard input"
file_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
