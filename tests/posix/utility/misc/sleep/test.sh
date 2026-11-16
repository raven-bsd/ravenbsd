#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup sleep

misc_status 0 "fractional sleep" "$UTIL" 0.01
misc_status 0 "zero sleep" "$UTIL" 0
misc_status nonzero "invalid interval" "$UTIL" not-a-time
misc_status nonzero "missing operand" "$UTIL"
