#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX make utility acceptance: variables, pattern/suffix rules, .PHONY,
# include, automatic variables, dry-run/silent, keep-going and failure.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

cargo build -q -p make
sh tests/posix/utility/make/test.sh
