#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-file.sh"
file_setup pwd

expected=$(cd "$TMPDIR" && pwd -P)
file_eq "$expected" "$(cd "$TMPDIR" && "$UTIL")" "physical working directory"
file_eq "$expected" "$(cd "$TMPDIR" && "$UTIL" -P)" "explicit -P"

ln -s "$TMPDIR" "$TMPDIR/link"
file_eq "$expected" "$(cd "$TMPDIR/link" && "$UTIL" -P)" "symlink resolved by -P"
