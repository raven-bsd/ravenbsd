#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup groups

out=$("$UTIL")
misc_nonempty "$out" "current groups"
misc_match '[[:alnum:]]' "$out" "group name"
misc_status 0 "named user" "$UTIL" "$(id -un)"
misc_status nonzero "unknown user" "$UTIL" no-such-p4f-user
