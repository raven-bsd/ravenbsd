#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for the first-party `grep` utility. The target binary needs the
# RavenBSD kernel; the same pattern selection is exercised by host tests.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p raven-grep
printf 'ok: raven-grep host tests\n'
