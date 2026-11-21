#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K408 start/restart the host peer before QEMU boots. Kill a stale peer so it cannot steal the
# one accepted connection; clear the per-run artifacts.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-write-exec
mode=${SOCKET_WRITE_EXEC_MODE:-connect}
if [ "$mode" = "accept" ]; then
    export SOCKET_WRITE_EXEC_HOST_MODE=connect
else
    export SOCKET_WRITE_EXEC_HOST_MODE=listen
fi
pkill -f "tests/fixtures/socket-write-exec/host.py" 2>/dev/null || true
rm -f "$out/host.log" "$out/host.result" "$out/host.stdout" "$out/qemu.pcap"
python3 tests/fixtures/socket-write-exec/host.py > "$out/host.stdout" 2>&1 &
