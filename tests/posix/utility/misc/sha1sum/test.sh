#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sha1sum

misc_eq "da39a3ee5e6b4b0d3255bfef95601890afd80709" \
    "$(printf '' | "$UTIL" | cut -d' ' -f1)" "empty digest"
printf 'abc' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/check"
misc_status 0 "-c check" "$UTIL" -c "$TMPDIR/check"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
