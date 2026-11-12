#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the host-side echo client before QEMU boots. It retries QEMU's hostfwd port until the
# guest listener is reachable, writes the exact-echo result for the second boot to verify, and
# exits. This is a test helper, not a build or QEMU process; the qtest stays in the foreground.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b70-net
mkdir -p "$out"
rm -f "$out/host-echo.result" "$out/host-echo.log" "$out/host-echo.pid"

python3 tests/fixtures/b70-net/host-echo.py \
    --host 127.0.0.1 \
    --port 23070 \
    --result "$out/host-echo.result" \
    --log "$out/host-echo.log" \
    > "$out/host-echo.stdout" 2>&1 &
echo $! > "$out/host-echo.pid"
