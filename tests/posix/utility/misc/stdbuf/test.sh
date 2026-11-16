#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup stdbuf

misc_eq "x" "$(printf 'x\n' | "$UTIL" -oL cat)" "-oL passthrough"
misc_eq "x" "$(printf 'x\n' | "$UTIL" -o0 cat)" "-o0 passthrough"
misc_eq "x" "$(printf 'x\n' | "$UTIL" -eL cat)" "-eL passthrough"
misc_status nonzero "missing command" "$UTIL" -oL
