#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup expr

text_eq "5" "$("$UTIL" 2 + 3)" "addition"
text_eq "14" "$("$UTIL" 2 + 3 '*' 4)" "precedence"
text_eq "1" "$("$UTIL" 10 % 3)" "modulo"
text_eq "3" "$("$UTIL" length abc)" "length"
text_eq "bcd" "$("$UTIL" substr abcdef 2 3)" "substr"
text_eq "3" "$("$UTIL" index abcdef cd)" "index"
text_eq "1" "$("$UTIL" 2 '<' 3)" "comparison"
text_status 1 "zero result" "$UTIL" 0
text_status 1 "empty result" "$UTIL" ''
text_status 2 "syntax error" "$UTIL" 1 +
