#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup timeout

misc_status 0 "command succeeds" "$UTIL" 1 sh -c 'exit 0'
misc_status 124 "command times out" "$UTIL" 0.1 sleep 5
misc_status 143 "--preserve-status" "$UTIL" --preserve-status 0.1 sleep 5
misc_status nonzero "missing command" "$UTIL" 1
