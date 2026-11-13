#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot UDP echo server used by the netd-virtio qtest. The qtest pre-hook waits for
# this shell to exit, so the server itself is backgrounded and exits after one datagram.

set -eu

PORT=${1:-47001}
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

# A failed previous run may still be waiting for its datagram.
pkill -f "b12-netd/udp_echo.py $PORT" 2>/dev/null || true
python3 "$HERE/udp_echo.py" "$PORT" >/tmp/b12c-udp-echo.log 2>&1 &
