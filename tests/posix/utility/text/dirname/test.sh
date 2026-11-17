#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup dirname

text_eq "/usr/bin" "$("$UTIL" /usr/bin/sort)" "path dirname"
text_eq "." "$("$UTIL" sort)" "relative name"
text_eq "/" "$("$UTIL" /)" "root"
text_eq "/usr" "$("$UTIL" /usr/bin/)" "trailing slashes"
text_status nonzero "missing operand" "$UTIL"
