#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup users

misc_status 0 "current users"
misc_status 0 "file operand" "$UTIL" /dev/null
misc_status nonzero "extra operand" "$UTIL" /dev/null extra
