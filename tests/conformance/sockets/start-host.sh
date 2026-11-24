#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the S174 host peer before QEMU boots. The hostfwd is ephemeral
# (tcp:127.0.0.1:0 -> guest:23100) and the peer reads QEMU's `info usernet`
# from the monitor socket to find the chosen host port; it retries until the
# listener appears.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=${SOCKET_CONFORMANCE_OUT:-build/fixtures/socket-conformance}
mkdir -p "$out"
rm -f "$out/host.result" "$out/host.log" "$out/host.stdout" "$out/monitor.sock"
pkill -f "$root/tests/conformance/sockets/host.py" 2>/dev/null || true
python3 "$root/tests/conformance/sockets/host.py" \
    --host 127.0.0.1 --monitor "$out/monitor.sock" \
    --result "$out/host.result" --log "$out/host.log" \
    > "$out/host.stdout" 2>&1 &
