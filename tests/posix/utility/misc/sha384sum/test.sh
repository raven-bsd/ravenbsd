#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sha384sum

misc_eq "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b" \
    "$(printf '' | "$UTIL" | cut -d' ' -f1)" "empty digest"
printf 'abc' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/check"
misc_status 0 "-c check" "$UTIL" -c "$TMPDIR/check"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
