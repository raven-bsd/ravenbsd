#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup truncate

printf 'abcdefghij' >"$TMPDIR/file"
"$UTIL" -s 4 "$TMPDIR/file"
misc_eq "4" "$(wc -c <"$TMPDIR/file")" "shrink to 4"
printf 'abcd' >"$TMPDIR/want"
misc_file_eq "$TMPDIR/want" "$TMPDIR/file" "shrink contents"
"$UTIL" -s 8 "$TMPDIR/file"
misc_eq "8" "$(wc -c <"$TMPDIR/file")" "extend to 8"
printf 'abcd\000\000\000\000' >"$TMPDIR/want2"
misc_file_eq "$TMPDIR/want2" "$TMPDIR/file" "extension is zero filled"
printf 'AAAA' >"$TMPDIR/ref"
"$UTIL" -r "$TMPDIR/ref" "$TMPDIR/file"
misc_eq "4" "$(wc -c <"$TMPDIR/file")" "-r reference"
"$UTIL" --no-create -s 4 "$TMPDIR/missing"
[ ! -e "$TMPDIR/missing" ] || misc_fail "--no-create created a file"
misc_ok "--no-create"
