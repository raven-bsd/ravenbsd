#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-system.sh"
system_setup stty

settings=$(system_pty "$UTIL -a")
case "$settings" in
    *speed*rows*) system_ok "settings include speed and rows" ;;
    *) system_fail "missing speed/rows in stty -a output" ;;
esac

size=$(system_pty "$UTIL size")
case "$size" in
    [0-9]*" "[0-9]*) system_ok "size is two numbers ($size)" ;;
    *) system_fail "unexpected stty size [$size]" ;;
esac

system_pty "$UTIL sane" >/dev/null
system_ok "sane on a pty"

set +e
printf '' | "$UTIL" -a >"$TMPDIR/non-tty.out" 2>"$TMPDIR/non-tty.err"
status=$?
set -e
system_eq 1 "$status" "non-tty exit status"
case "$(cat "$TMPDIR/non-tty.err")" in
    *"Inappropriate ioctl"*) system_ok "non-tty diagnostic" ;;
    *) system_fail "unexpected non-tty diagnostic" ;;
esac
