#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-system.sh"
system_setup tty

name=$(system_pty "$UTIL")
case "$name" in
    /dev/*) system_ok "pty name ($name)" ;;
    *) system_fail "unexpected tty name [$name]" ;;
esac

system_pty "$UTIL -s" >/dev/null
system_ok "silent option on a tty"

set +e
printf '' | "$UTIL" >"$TMPDIR/non-tty.out" 2>"$TMPDIR/non-tty.err"
status=$?
set -e
system_eq 1 "$status" "non-tty exit status"
system_eq "not a tty" "$(cat "$TMPDIR/non-tty.out")" "non-tty message"

set +e
printf '' | "$UTIL" -s >/dev/null 2>&1
status=$?
set -e
system_eq 1 "$status" "silent non-tty exit status"
