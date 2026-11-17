#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup mktemp

file=$("$UTIL")
[ -f "$file" ] || misc_fail "mktemp did not create a file"
misc_ok "file template"
rm -f "$file"

dir=$("$UTIL" -d)
[ -d "$dir" ] || misc_fail "mktemp -d did not create a directory"
misc_ok "-d directory"
rmdir "$dir"

custom=$("$UTIL" --tmpdir="$TMPDIR" p4f.XXXXXX)
case $custom in
    "$TMPDIR"/p4f.*) misc_ok "custom template" ;;
    *) misc_fail "custom template path: $custom" ;;
esac
rm -f "$custom"
misc_status nonzero "unknown option" "$UTIL" --not-an-option
