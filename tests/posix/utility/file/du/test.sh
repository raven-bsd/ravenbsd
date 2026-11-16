#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup du

mkdir -p "$TMPDIR/d/sub"
printf 'hello\n' >"$TMPDIR/d/a"
printf 'world\n' >"$TMPDIR/d/sub/b"

out=$("$UTIL" -s "$TMPDIR/d")
first=$(printf '%s\n' "$out" | cut -f1)
case "$first" in
    '' | *[!0-9]*) file_fail "'du -s' did not start with a byte count: [$out]" ;;
    *) file_ok "'du -s' byte count" ;;
esac
file_contains "$out" "$TMPDIR/d" "summarised path"

out=$("$UTIL" -a "$TMPDIR/d")
file_contains "$out" "a" "file entry"
file_contains "$out" "sub/b" "nested entry"
file_status nonzero "missing path" "$UTIL" "$TMPDIR/missing"
