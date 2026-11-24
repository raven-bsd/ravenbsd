#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K431 start/restart the S5 host peer before QEMU boots. Kill a stale peer so it cannot steal the
# one accepted connection; clear the per-run artifacts.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-write-identity
mkdir -p "$out"
pkill -f "tests/fixtures/socket-write-identity/host.py" 2>/dev/null || true
rm -f "$out/host.log" "$out/host.result" "$out/host.stdout" "$out/ssh.pcap"
python3 tests/fixtures/socket-write-identity/host.py \
    --mode connect \
    --host 10.43.0.15 \
    --port 23097 \
    --log "$out/host.log" \
    --result "$out/host.result" > "$out/host.stdout" 2>&1 &
