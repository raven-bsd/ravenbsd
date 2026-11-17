#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P40 acceptance: at/batch host cases and the atd qtest.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

sh tests/posix/utility/at/test.sh
cargo xtask qtest --case atd --arch x86_64
