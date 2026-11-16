#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup ls

printf 'a\n' >"$TMPDIR/file"
printf 'b\n' >"$TMPDIR/.hidden"
mkdir "$TMPDIR/dir"
printf 'c\n' >"$TMPDIR/dir/c"

out=$("$UTIL" -1 "$TMPDIR")
file_contains "$out" "file" "single-column name"
file_contains "$out" "dir" "directory name"
file_not_contains "$out" ".hidden" "hidden omitted by default"

out=$("$UTIL" -a1 "$TMPDIR")
file_contains "$out" ".hidden" "hidden shown with -a"

out=$("$UTIL" -d "$TMPDIR")
file_eq "$TMPDIR" "$out" "directory itself with -d"

out=$("$UTIL" -l "$TMPDIR/file")
file_contains "$out" "file" "long listing name"
file_status nonzero "missing path" "$UTIL" "$TMPDIR/missing"
