#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup factor

misc_eq "12: 2 2 3" "$("$UTIL" 12)" "factor 12"
misc_eq "97: 97" "$("$UTIL" 97)" "prime factor"
misc_eq "$(printf '10: 2 5\n15: 3 5')" \
    "$(printf '10\n15\n' | "$UTIL")" "standard input"
misc_status nonzero "invalid number" "$UTIL" not-a-number
