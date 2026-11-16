#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup seq

misc_eq "$(printf '1\n2\n3')" "$("$UTIL" 3)" "last"
misc_eq "$(printf '1\n3\n5')" "$("$UTIL" 1 2 5)" "first increment last"
misc_eq "$(printf '08\n09\n10')" "$("$UTIL" -w 8 10)" "-w equal width"
misc_eq "1,2,3" "$("$UTIL" -s, 1 3)" "-s separator"
misc_eq "$(printf '3\n2\n1')" "$("$UTIL" 3 -1 1)" "descending"
misc_status nonzero "zero increment" "$UTIL" 1 0 3
