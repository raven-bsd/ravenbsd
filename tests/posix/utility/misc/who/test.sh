#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup who

misc_status 0 "current users"
misc_status 0 "am i" "$UTIL" am i
misc_match '# user=[0-9]+' "$("$UTIL" -q)" "-q user count"
misc_status nonzero "bad option" "$UTIL" --not-an-option
