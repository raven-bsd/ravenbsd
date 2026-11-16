#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup tr

text_eq "ABC" "$(printf 'abc\n' | "$UTIL" '[:lower:]' '[:upper:]')" "character class"
text_eq "xyz" "$(printf 'abc\n' | "$UTIL" abc xyz)" "SET1 to SET2"
text_eq "ab" "$(printf 'a1b2\n' | "$UTIL" -d 0-9)" "-d delete"
text_eq "a b" "$(printf 'a   b\n' | "$UTIL" -s ' ')" "-s squeeze"
text_eq "abXXcd" "$(printf 'ab12cd' | "$UTIL" -c 'a-z' X)" "-c complement"
text_eq "xxx" "$(printf 'abc' | "$UTIL" abc x)" "short SET2 padding"
printf 'a\000b' >"$TMPDIR/nul"
text_eq "ab" "$("$UTIL" -d '\000' <"$TMPDIR/nul")" "-d NUL byte"
text_status nonzero "invalid character class" \
    sh -c 'printf abc | "$1" "[:bogus:]" x' sh "$UTIL"
