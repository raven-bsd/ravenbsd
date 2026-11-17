#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup head

printf '%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 >"$TMPDIR/in"
printf '%s\n' 1 2 3 4 5 6 7 8 9 10 >"$TMPDIR/first10"

"$UTIL" "$TMPDIR/in" >"$TMPDIR/out"
text_file_eq "$TMPDIR/first10" "$TMPDIR/out" "default 10 lines"
text_eq "$(printf '1\n2')" "$("$UTIL" -n 2 "$TMPDIR/in")" "-n 2"
text_eq "$(printf '1\n2\n3\n4\n5\n6\n7\n8\n9\n10')" \
    "$("$UTIL" -n -2 "$TMPDIR/in")" "-n -2 all but last two"
text_eq "$(printf '1\n2\n3')" "$("$UTIL" -c 5 "$TMPDIR/in")" "-c 5 bytes"
text_eq "$(printf '1\n2')" "$("$UTIL" -n 2 <"$TMPDIR/in")" "standard input"
text_eq "$(printf '==> %s <==\n1\n\n==> %s <==\n1' "$TMPDIR/in" "$TMPDIR/in")" \
    "$("$UTIL" -n 1 "$TMPDIR/in" "$TMPDIR/in")" "multiple file headers"
printf 'a\000b\n' >"$TMPDIR/nul"
printf 'a\000b' >"$TMPDIR/want-nul"
"$UTIL" -c 3 "$TMPDIR/nul" >"$TMPDIR/out-nul"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/out-nul" "NUL byte in byte count"
text_status nonzero "missing file" "$UTIL" -n 1 "$TMPDIR/missing"
