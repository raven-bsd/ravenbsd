#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup false

misc_status 1 "exit status" "$UTIL"
misc_status 1 "extra operands ignored" "$UTIL" extra words
