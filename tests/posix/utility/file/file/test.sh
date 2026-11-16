#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for the first-party `file` utility. The target binary needs the
# RavenBSD kernel; the same classification code is exercised by host unit and filesystem tests.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p raven-file
printf 'ok: raven-file host tests\n'
