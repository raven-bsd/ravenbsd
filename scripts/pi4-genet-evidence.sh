#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture and check a Pi 4 GENET serial log. This is an evidence helper, not a board driver:
# it consumes the UART log that the board owner captures and fails when a required GENET marker
# is absent. It never changes the repository.
#
# Usage:
#   scripts/pi4-genet-evidence.sh <serial.log>
#   scripts/pi4-genet-evidence.sh --capture <tty> <serial.log> [seconds]
#
# The marker names are documented in docs/pi4-genet-board.md.

set -eu

usage() {
    echo "usage: $0 <serial.log>" >&2
    echo "       $0 --capture <tty> <serial.log> [seconds]" >&2
    exit 2
}

log=
if [ "${1:-}" = "--capture" ]; then
    [ "$#" -ge 3 ] || usage
    device=$2
    log=$3
    seconds=${4:-60}
    command -v stty >/dev/null 2>&1 || { echo "stty not found" >&2; exit 2; }
    command -v timeout >/dev/null 2>&1 || { echo "timeout not found" >&2; exit 2; }
    stty -F "$device" 115200 cs8 -cstopb -parenb -crtscts
    timeout "$seconds" cat "$device" >"$log" || true
elif [ "$#" -eq 1 ]; then
    log=$1
else
    usage
fi

[ -s "$log" ] || { echo "genet-evidence: log is empty or missing: $log" >&2; exit 1; }

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || printf 'unknown')
if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$log" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    hash=$(shasum -a 256 "$log" | awk '{print $1}')
else
    echo "genet-evidence: sha256sum or shasum is required" >&2
    exit 2
fi

echo "genet-evidence: commit=$commit"
echo "genet-evidence: log=$log sha256=$hash"

missing=0
for marker in GENET-REV GENET-PORT GENET-MAC GENET-UMAC GENET-PHY GENET-RGMII; do
    if ! grep -Fq -- "$marker" "$log"; then
        echo "genet-evidence: missing marker $marker"
        missing=1
    fi
done

if [ "$missing" -ne 0 ]; then
    echo "genet-evidence: FAIL"
    exit 1
fi

echo "genet-evidence: PASS"
