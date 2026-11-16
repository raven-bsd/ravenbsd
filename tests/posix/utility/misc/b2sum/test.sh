#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup b2sum

empty=786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce
misc_eq "$empty" "$(printf '' | "$UTIL" | cut -d' ' -f1)" "empty digest"
printf 'abc' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/check"
misc_match '^[0-9a-f]{128}  ' "$(cat "$TMPDIR/check")" "128-bit file digest"
misc_status 0 "-c check" "$UTIL" -c "$TMPDIR/check"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
