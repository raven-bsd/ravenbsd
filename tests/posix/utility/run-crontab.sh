#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P4y acceptance: crontab host cases and the crond qtest.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

sh tests/posix/utility/crontab/test.sh
cargo xtask qtest --case crond --arch x86_64
