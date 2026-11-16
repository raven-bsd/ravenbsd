#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture and check the Pi 4 release network evidence. This is an evidence helper, not a
# driver: it consumes the serial log the board owner captured and fails when the link, DHCP,
# gateway ping or completion marker is absent. It never changes the repository.
#
# Usage:
#   scripts/pi4-net-evidence.sh <serial.log>
#   scripts/pi4-net-evidence.sh --capture <tty> <serial.log> [seconds]
#   scripts/pi4-net-evidence.sh --self-test
#
# The marker contract is documented in docs/pi4-net.md.

set -eu

usage() {
    echo "usage: $0 <serial.log>" >&2
    echo "       $0 --capture <tty> <serial.log> [seconds]" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

script=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

self_test() {
    positive=$repo/tests/fixtures/pi4-net/model-transcript.log
    negative=$repo/tests/fixtures/pi4-net/model-missing-ping.log
    [ -s "$positive" ] || { echo "pi4-net-evidence: missing $positive" >&2; exit 1; }
    [ -s "$negative" ] || { echo "pi4-net-evidence: missing $negative" >&2; exit 1; }
    sh "$script" "$positive" >/dev/null
    if sh "$script" "$negative" >/dev/null 2>&1; then
        echo "pi4-net-evidence: self-test accepted a log without a ping" >&2
        exit 1
    fi
    echo "pi4-net-evidence: self-test PASS"
}

if [ "${1:-}" = "--self-test" ]; then
    [ "$#" -eq 1 ] || usage
    self_test
    exit 0
fi

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

[ -s "$log" ] || { echo "pi4-net-evidence: log is empty or missing: $log" >&2; exit 1; }

commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || printf 'unknown')
if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$log" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    hash=$(shasum -a 256 "$log" | awk '{print $1}')
else
    echo "pi4-net-evidence: sha256sum or shasum is required" >&2
    exit 2
fi

echo "pi4-net-evidence: commit=$commit"
echo "pi4-net-evidence: log=$log sha256=$hash"

missing=0
check() {
    if ! grep -Eq -- "$1" "$log"; then
        echo "pi4-net-evidence: missing $2" >&2
        missing=1
    fi
}

check '^netd: interface eth0 [0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}/[0-9]{1,2}$' \
    'the netd eth0 lease line'
check '^ping: .* packets transmitted, [1-9][0-9]* received, [0-9]+% packet loss$' \
    'a ping summary with at least one reply'
check '^GENET-NET-LINK speed=(10|100|1000) duplex=(full|half)$' \
    'the GENET-NET-LINK marker'
check '^GENET-NET-DHCP addr=[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}/[0-9]{1,2} gateway=[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3} dns=(none|[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3})$' \
    'the GENET-NET-DHCP marker'
check '^GENET-NET-PING target=[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3} tx=[0-9]+ rx=[0-9]+ rtt_ms=[0-9]+$' \
    'the GENET-NET-PING marker'
check '^GENET-NET-OK$' 'the GENET-NET-OK marker'

if ! grep -Fq -- 'GENET-NET-OK' "$log"; then
    echo "pi4-net-evidence: the gate did not complete" >&2
    missing=1
fi

if ! awk '
    $1 == "GENET-NET-PING" {
        for (i = 1; i <= NF; i++) {
            if ($i ~ /^rx=/) {
                split($i, value, "=")
                if (value[2] + 0 > 0) found = 1
            }
        }
    }
    END { exit(found ? 0 : 1) }
' "$log"; then
    echo "pi4-net-evidence: the gateway ping received no reply" >&2
    missing=1
fi

if [ "$missing" -ne 0 ]; then
    echo "pi4-net-evidence: FAIL"
    exit 1
fi

echo "pi4-net-evidence: PASS"
