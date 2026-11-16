#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup tail

printf '%s\n' 1 2 3 4 5 6 7 8 9 10 11 12 >"$TMPDIR/in"
printf '%s\n' 3 4 5 6 7 8 9 10 11 12 >"$TMPDIR/last10"

"$UTIL" "$TMPDIR/in" >"$TMPDIR/out"
text_file_eq "$TMPDIR/last10" "$TMPDIR/out" "default 10 lines"
text_eq "$(printf '11\n12')" "$("$UTIL" -n 2 "$TMPDIR/in")" "-n 2"
text_eq "$(printf '10\n11\n12')" "$("$UTIL" -n +10 "$TMPDIR/in")" "-n +10"
text_eq "$(printf '\n12')" "$("$UTIL" -c 4 "$TMPDIR/in")" "-c 4 bytes"
text_eq "$(printf '11\n12')" "$("$UTIL" -n 2 <"$TMPDIR/in")" "standard input"
text_eq "$(printf '==> %s <==\n12\n\n==> %s <==\n12' "$TMPDIR/in" "$TMPDIR/in")" \
    "$("$UTIL" -n 1 "$TMPDIR/in" "$TMPDIR/in")" "multiple file headers"
printf 'a\000b\n' >"$TMPDIR/nul"
printf 'a\000b\n' >"$TMPDIR/want-nul"
"$UTIL" -c 4 "$TMPDIR/nul" >"$TMPDIR/out-nul"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/out-nul" "NUL byte in byte count"
text_status nonzero "missing file" "$UTIL" -n 1 "$TMPDIR/missing"
