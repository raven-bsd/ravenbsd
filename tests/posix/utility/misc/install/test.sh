#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup install

printf 'hello\n' >"$TMPDIR/src"
mkdir "$TMPDIR/out"
"$UTIL" -m 755 "$TMPDIR/src" "$TMPDIR/out/dst"
misc_file_eq "$TMPDIR/src" "$TMPDIR/out/dst" "copy contents"
misc_eq "755" "$(stat -c %a "$TMPDIR/out/dst")" "mode"
"$UTIL" -d -m 700 "$TMPDIR/out/dir"
[ -d "$TMPDIR/out/dir" ] || misc_fail "install -d did not create a directory"
misc_eq "700" "$(stat -c %a "$TMPDIR/out/dir")" "-d mode"
misc_status nonzero "missing source" "$UTIL" "$TMPDIR/missing" "$TMPDIR/out/x"
