#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup arch

misc_match '^[a-z0-9_]+$' "$("$UTIL")" "machine architecture"
misc_status 0 "--help" "$UTIL" --help
misc_status nonzero "extra operand" "$UTIL" extra
