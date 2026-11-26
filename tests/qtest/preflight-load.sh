#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Rev 111 load preflight for the kernel-timer row.
#
# The 30-run distribution was 98-101 ms at load1 3.10-4.59 (30/30 inside 90-119); the preserved
# full-gate failures are 126 ms at load1 6.32 and 146 ms at load1 60.83. The safe upper bound is
# ceil(4.59) = 5.0, which is 1.32 below the first observed failure. Refuse above 5.0 with exit 77
# so the gate can report waiting-for-window instead of a timer failure. This is a load guard, not
# a timer budget: the 90-130 ms qtest window is unchanged.

set -u

max_load=5.0
load1=

usage() {
    cat <<'EOF'
usage: sh tests/qtest/preflight-load.sh [--load1 VALUE] [--max-load VALUE]

  --load1 VALUE     override /proc/loadavg (synthetic tests only)
  --max-load VALUE  safe 1-minute load bound (default 5.0)
  -h, --help        show this help

Exit 0 when load1 <= max-load; exit 77 when above (waiting for a host window).
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --load1)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            load1=$2
            shift 2
            ;;
        --max-load)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            max_load=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "preflight-load: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$load1" ]; then
    if [ ! -r /proc/loadavg ]; then
        echo "preflight-load: /proc/loadavg is not readable" >&2
        exit 2
    fi
    load1=$(awk '{print $1}' /proc/loadavg)
fi

case "$load1" in
    ''|*[!0-9.]*)
        echo "preflight-load: invalid load1: $load1" >&2
        exit 2
        ;;
esac
case "$max_load" in
    ''|*[!0-9.]*)
        echo "preflight-load: invalid max-load: $max_load" >&2
        exit 2
        ;;
esac

if awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l > m) }'; then
    echo "preflight-load: load1=$load1 max=$max_load waiting-for-window"
    exit 77
fi

echo "preflight-load: load1=$load1 max=$max_load ok"
exit 0
