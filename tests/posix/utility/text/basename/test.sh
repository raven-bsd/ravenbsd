#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup basename

text_eq "sort" "$("$UTIL" /usr/bin/sort)" "path basename"
text_eq "foo" "$("$UTIL" foo.txt .txt)" "suffix removal"
text_eq "bin" "$("$UTIL" /usr/bin/)" "trailing slashes"
text_eq "sort" "$("$UTIL" sort)" "relative name"
text_eq "-x" "$("$UTIL" -- -x)" "end of options"
text_status nonzero "extra operand" "$UTIL" a b c
