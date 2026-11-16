#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup basenc

printf 'hello\n' >"$TMPDIR/in"
misc_eq "NBSWY3DPBI======" "$("$UTIL" --base32 <"$TMPDIR/in")" "--base32"
misc_eq "aGVsbG8K" "$("$UTIL" --base64 <"$TMPDIR/in")" "--base64"
misc_eq "68656C6C6F0A" "$("$UTIL" --base16 <"$TMPDIR/in")" "--base16"
"$UTIL" --base64 <"$TMPDIR/in" >"$TMPDIR/encoded"
"$UTIL" --base64 -d <"$TMPDIR/encoded" >"$TMPDIR/decoded"
misc_file_eq "$TMPDIR/in" "$TMPDIR/decoded" "--base64 decode"
misc_status nonzero "missing encoding" "$UTIL"
