#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup numfmt

misc_eq "1.5k" "$("$UTIL" --to=si 1500)" "--to=si"
misc_eq "1024" "$("$UTIL" --from=iec 1K)" "--from=iec"
misc_eq "1.0Ki" "$("$UTIL" --to=iec-i 1024)" "--to=iec-i"
misc_eq "1.0k" "$(printf '1000\n' | "$UTIL" --to=si)" "standard input"
misc_status nonzero "invalid number" "$UTIL" --to=si not-a-number
