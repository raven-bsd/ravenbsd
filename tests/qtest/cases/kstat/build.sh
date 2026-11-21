#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# KSTAT1 test-first gate. The three product surfaces must exist before the case can build and
# boot. Until the read-path/kernel-registration Owns are granted this fails with the exact missing
# paths; see tickets/tim/reports/KSTAT1.md.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

missing=""
for path in \
    kernel/src/kstat \
    crates/raven-kstat \
    userland/bin/kstat
do
    if [ ! -e "$path" ]; then
        missing="$missing $path"
    fi
done
if [ -n "$missing" ]; then
    printf 'kstat: KSTAT1 product missing:%s\n' "$missing" >&2
    printf 'kstat: read-path/kernel-registration Owns not granted; see tickets/tim/reports/KSTAT1.md\n' >&2
    exit 1
fi
