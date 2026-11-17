#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b time behavior evidence:
#   * the raven-libc-time unit suite (asctime, strftime, wcsftime, clocks, sleeps);
#   * a defined-symbol check for every claimed time row.
#
# The C-ABI execution of these shell wrappers needs the RavenOS process/thread setup; the
# host-side C harness cannot initialise the shell platform seam, so the engine tests and the
# archive symbol check are the evidence for this slice.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-time

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix time: $archive is missing" >&2; exit 1; }

for symbol in \
    asctime clock_nanosleep ctime stime strftime_l strptime_l \
    timespec_get timespec_getres tzset wcsftime
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix time: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix time: PASS"
