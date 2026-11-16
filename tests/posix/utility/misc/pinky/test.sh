#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup pinky

out=$("$UTIL")
misc_match '^Login' "$out" "header"
misc_status 0 "named user" "$UTIL" "$(id -un)"
misc_status 0 "unknown user is empty" "$UTIL" no-such-p4f-user
