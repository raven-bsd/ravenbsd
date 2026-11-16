#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for the first-party `find` utility. The target binary needs the
# RavenBSD kernel; the same traversal code is exercised against a real host tree.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p raven-find
printf 'ok: raven-find host tests\n'
