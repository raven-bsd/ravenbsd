#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup shuf

misc_eq "3" "$("$UTIL" -e a b c | wc -l)" "-e line count"
misc_eq "$(printf 'a\nb\nc')" "$("$UTIL" -e a b c | sort)" "-e shuffled set"
misc_eq "2" "$("$UTIL" -n 2 -i 1-5 | wc -l)" "-n 2 count"
misc_match '^[1-5]$' "$("$UTIL" -n 1 -i 1-5)" "-i range"
misc_status nonzero "bad range" "$UTIL" -i 5-1
