#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup base64

printf 'hello\n' >"$TMPDIR/in"
misc_eq "aGVsbG8K" "$("$UTIL" <"$TMPDIR/in")" "encode hello"
"$UTIL" <"$TMPDIR/in" >"$TMPDIR/encoded"
"$UTIL" -d <"$TMPDIR/encoded" >"$TMPDIR/decoded"
misc_file_eq "$TMPDIR/in" "$TMPDIR/decoded" "decode round trip"
misc_status nonzero "invalid decode" sh -c 'printf "!!!" | "$1" -d' sh "$UTIL"
misc_status 0 "file input" "$UTIL" "$TMPDIR/in"
