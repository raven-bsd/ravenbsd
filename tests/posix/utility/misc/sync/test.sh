#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sync

printf 'data\n' >"$TMPDIR/file"
misc_status 0 "file operand" "$UTIL" "$TMPDIR/file"
misc_status 0 "-d data" "$UTIL" -d "$TMPDIR/file"
misc_status 0 "all filesystems" "$UTIL"
