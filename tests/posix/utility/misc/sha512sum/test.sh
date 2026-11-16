#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sha512sum

misc_eq "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e" \
    "$(printf '' | "$UTIL" | cut -d' ' -f1)" "empty digest"
printf 'abc' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/check"
misc_status 0 "-c check" "$UTIL" -c "$TMPDIR/check"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
