#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture and check the Pi 4 release network evidence. This is an evidence helper, not a
# driver: it consumes the serial log the board owner captured and fails when the final launch,
# link, DHCP, gateway ping or completion marker is absent or inconsistent. It never changes the
# repository.
#
# Usage:
#   scripts/pi4-net-evidence.sh <serial.log>
#   scripts/pi4-net-evidence.sh --capture <tty> <serial.log> [seconds]
#   scripts/pi4-net-evidence.sh --chain <serial.log>
#   scripts/pi4-net-evidence.sh --self-test
#
# The marker contract and the failure modes are documented in docs/pi4-net.md.

set -eu

usage() {
    echo "usage: $0 <serial.log>" >&2
    echo "       $0 --capture <tty> <serial.log> [seconds]" >&2
    echo "       $0 --chain <serial.log>" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

script=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
chain_check="$repo/tests/fixtures/pi4-net/check-chain.sh"

self_test() {
    positive=$repo/tests/fixtures/pi4-net/model-transcript.log
    negatives="
        model-missing-ping.log
        model-static-qemu.log
        model-missing-lease.log
        model-wrong-gateway.log
        model-link-down.log
        model-duplicate-launch.log
    "
    [ -s "$positive" ] || { echo "pi4-net-evidence: missing $positive" >&2; exit 1; }
    sh "$script" "$positive" >/dev/null
    for name in $negatives; do
        negative=$repo/tests/fixtures/pi4-net/$name
        [ -s "$negative" ] || { echo "pi4-net-evidence: missing $negative" >&2; exit 1; }
        if sh "$script" "$negative" >/dev/null 2>&1; then
            echo "pi4-net-evidence: self-test accepted $name" >&2
            exit 1
        fi
    done
    sh "$script" --chain "$positive" >/dev/null
    if sh "$script" --chain "$repo/tests/fixtures/pi4-net/model-missing-ping.log" >/dev/null 2>&1; then
        echo "pi4-net-evidence: self-test accepted a chained missing-ping log" >&2
        exit 1
    fi
    echo "pi4-net-evidence: self-test PASS"
}

if [ "${1:-}" = "--self-test" ]; then
    [ "$#" -eq 1 ] || usage
    self_test
    exit 0
fi

chain=0
log=
if [ "${1:-}" = "--chain" ]; then
    [ "$#" -eq 2 ] || usage
    chain=1
    log=$2
elif [ "${1:-}" = "--capture" ]; then
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

if [ "$chain" -eq 1 ]; then
    if ! sh "$chain_check"; then
        echo "pi4-net-evidence: chain check failed" >&2
        exit 1
    fi
fi

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

forbid() {
    if grep -Eq -- "$1" "$log"; then
        echo "pi4-net-evidence: forbidden $2" >&2
        missing=1
    fi
}

count_eq() {
    count=$(grep -Ec -- "$1" "$log" || true)
    if [ "$count" -ne "$2" ]; then
        echo "pi4-net-evidence: expected $2 of $3, found $count" >&2
        missing=1
    fi
}

check '^devd: dtb launch genet bar=[0-9]+ size=0x[0-9a-f]+ dma=[0-9]+ irq=[0-9]+$' \
    'the devd GENET launch line'
count_eq '^devd: dtb launch genet ' 1 'the devd GENET launch line'
count_eq '^genet: revision=0x[0-9a-f]+ mac=.* slots=Platform$' 1 'the GENET start line'
count_eq '^genet: serving nic:$' 1 'the single nic: server line'

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
count_eq '^GENET-NET-OK$' 1 'the GENET-NET-OK marker'

forbid '10\.0\.2\.15/24|gateway=10\.0\.2\.2' 'the QEMU user-mode address on the Pi'
forbid '^GENET-NET-LINK .*down' 'a link-down marker'

if ! awk '
    /^GENET-NET-LINK speed=/ && !link { link=NR }
    /^GENET-NET-DHCP addr=/ && !dhcp { dhcp=NR }
    /^GENET-NET-PING target=/ && !ping { ping=NR }
    /^GENET-NET-OK$/ && !ok { ok=NR }
    END { exit !(link && dhcp && ping && ok && link < dhcp && dhcp < ping && ping < ok) }
' "$log"; then
    echo "pi4-net-evidence: marker sequence is incomplete or out of order" >&2
    missing=1
fi

if ! awk '
    /^netd: interface eth0 / {
        split($4, value, "/")
        netd_addr = value[1]
        netd_prefix = value[2]
    }
    /^GENET-NET-DHCP addr=/ {
        for (field = 1; field <= NF; field++) {
            if ($field ~ /^addr=/) {
                split($field, value, "=")
                split(value[2], parts, "/")
                dhcp_addr = parts[1]
                dhcp_prefix = parts[2]
            }
            if ($field ~ /^gateway=/) {
                split($field, value, "=")
                dhcp_gateway = value[2]
            }
        }
    }
    /^GENET-NET-PING target=/ {
        for (field = 1; field <= NF; field++) {
            if ($field ~ /^target=/) {
                split($field, value, "=")
                ping_target = value[2]
            }
        }
    }
    /^ping: / && /packets transmitted/ {
        ping_host = $2
    }
    END {
        if (netd_addr == "" || dhcp_addr == "" || dhcp_gateway == "" || ping_target == "" || ping_host == "") exit 1
        if (netd_addr != dhcp_addr || netd_prefix != dhcp_prefix) exit 1
        if (ping_target != dhcp_gateway || ping_host != dhcp_gateway) exit 1
    }
' "$log"; then
    echo "pi4-net-evidence: lease, gateway or ping target disagree" >&2
    missing=1
fi

if ! awk '
    $1 == "GENET-NET-PING" {
        for (field = 1; field <= NF; field++) {
            if ($field ~ /^rx=/) {
                split($field, value, "=")
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
