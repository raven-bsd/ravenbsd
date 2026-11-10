#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Optional external-oracle syntax check: run packetdrill's own parser (--dry_run) over our
# scripts. packetdrill is GPL-2.0 and stays a separate tool; nothing is linked or copied.
# The check skips cleanly when no binary is available.
set -eu

cd "$(dirname "$0")/../.."

if [ -n "${PACKETDRILL_BIN:-}" ]; then
    packetdrill=$PACKETDRILL_BIN
else
    packetdrill=../../../ravenbsd-resources/packetdrill/gtests/net/packetdrill/packetdrill
fi

if [ ! -x "$packetdrill" ]; then
    echo "packetdrill syntax check: skipped (set PACKETDRILL_BIN to an executable)"
    exit 0
fi

log=$(mktemp)
trap 'rm -f "$log"' EXIT INT TERM

status=0
for script in tests/net/scripts/*.pkt; do
    printf '%-28s ' "$(basename "$script")"
    if "$packetdrill" --dry_run "$script" >"$log" 2>&1; then
        echo OK
    else
        echo FAIL
        cat "$log"
        status=1
    fi
done
exit "$status"
