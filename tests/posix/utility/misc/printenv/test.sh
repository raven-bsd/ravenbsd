#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-misc.sh"
misc_setup printenv

misc_eq "bar" "$(FOO=bar "$UTIL" FOO)" "named variable"
misc_match '^FOO=bar$' "$(FOO=bar "$UTIL" | grep '^FOO=')" "environment listing"
misc_eq "bar baz" "$(FOO='bar baz' "$UTIL" FOO)" "space-preserving value"
misc_status 1 "missing variable" "$UTIL" NO_SUCH_P4F_VARIABLE
