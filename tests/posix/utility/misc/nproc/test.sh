#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup nproc

count=$("$UTIL")
misc_match '^[1-9][0-9]*$' "$count" "processor count"
misc_match '^[1-9][0-9]*$' "$("$UTIL" --all)" "--all processor count"
misc_status nonzero "extra operand" "$UTIL" extra
