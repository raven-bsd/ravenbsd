#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sha256sum

misc_eq "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" \
    "$(printf '' | "$UTIL" | cut -d' ' -f1)" "empty digest"
printf 'abc' >"$TMPDIR/in"
"$UTIL" "$TMPDIR/in" >"$TMPDIR/check"
misc_status 0 "-c check" "$UTIL" -c "$TMPDIR/check"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
