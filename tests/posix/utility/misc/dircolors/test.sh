#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup dircolors

misc_match '^LS_COLORS=' "$("$UTIL")" "default LS_COLORS"
misc_match '^export LS_COLORS' "$("$UTIL")" "shell export"
misc_status 0 "-b output" "$UTIL" -b
misc_status nonzero "unknown option" "$UTIL" --not-an-option
