#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup split

printf 'a\nb\nc\n' >"$TMPDIR/all"
printf 'abcdefghij' >"$TMPDIR/bytes"
mkdir "$TMPDIR/lines" "$TMPDIR/five" "$TMPDIR/suffix"

(cd "$TMPDIR/lines" && "$UTIL" -l 2 "$TMPDIR/all")
printf 'a\nb\n' >"$TMPDIR/want1"
printf 'c\n' >"$TMPDIR/want2"
text_file_eq "$TMPDIR/want1" "$TMPDIR/lines/xaa" "-l 2 first file"
text_file_eq "$TMPDIR/want2" "$TMPDIR/lines/xab" "-l 2 second file"

(cd "$TMPDIR/five" && "$UTIL" -b 5 "$TMPDIR/bytes")
printf 'abcde' >"$TMPDIR/want3"
printf 'fghij' >"$TMPDIR/want4"
text_file_eq "$TMPDIR/want3" "$TMPDIR/five/xaa" "-b 5 first file"
text_file_eq "$TMPDIR/want4" "$TMPDIR/five/xab" "-b 5 second file"

(cd "$TMPDIR/suffix" && "$UTIL" -b 5 -a 3 "$TMPDIR/bytes")
text_file_eq "$TMPDIR/want3" "$TMPDIR/suffix/xaaa" "-a 3 suffix length"
text_file_eq "$TMPDIR/want4" "$TMPDIR/suffix/xaab" "-a 3 second suffix"

mkdir "$TMPDIR/stdin"
(cd "$TMPDIR/stdin" && "$UTIL" -l 1 <"$TMPDIR/all")
printf 'a\n' >"$TMPDIR/want5"
text_file_eq "$TMPDIR/want5" "$TMPDIR/stdin/xaa" "standard input"
text_status nonzero "missing input" "$UTIL" "$TMPDIR/missing"
