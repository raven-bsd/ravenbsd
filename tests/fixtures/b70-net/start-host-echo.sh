#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot host echo client before QEMU boots. It retries QEMU's
# hostfwd port until the guest listener is reachable, verifies the exact
# payload, sends the ack and exits. The qtest itself stays in the foreground.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b70-net
mkdir -p "$out"
rm -f "$out/host-echo.result" "$out/host-echo.log" "$out/host-echo.stdout"

# A failed previous run may still be waiting for the guest.
pkill -f "b70-net/host-echo.py" 2>/dev/null || true

python3 tests/fixtures/b70-net/host-echo.py \
    --host 127.0.0.1 \
    --port 23070 \
    --result "$out/host-echo.result" \
    --log "$out/host-echo.log" \
    --timeout 240 \
    > "$out/host-echo.stdout" 2>&1 &
