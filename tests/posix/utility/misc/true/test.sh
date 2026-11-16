#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup true

misc_status 0 "exit status"
misc_status 0 "--help" "$UTIL" --help
misc_status 0 "extra operands ignored" "$UTIL" extra words
