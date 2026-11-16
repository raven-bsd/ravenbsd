#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup shred

printf 'secret\n' >"$TMPDIR/file"
"$UTIL" -n 1 -z "$TMPDIR/file"
misc_eq "4096" "$(wc -c <"$TMPDIR/file")" "-z zero padding"
if od -An -tx1 "$TMPDIR/file" | grep -q '[1-9a-f]'; then
    misc_fail "shred left non-zero bytes"
fi
misc_ok "zeroed contents"
printf 'secret\n' >"$TMPDIR/remove"
"$UTIL" -u "$TMPDIR/remove"
[ ! -e "$TMPDIR/remove" ] || misc_fail "-u left the file"
misc_ok "-u removal"
misc_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
