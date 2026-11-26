#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot K230 host echo client before QEMU boots. It retries the hostfwd port until
# the guest listener accepts, verifies the exact echo and sends the acknowledgement. The qtest
# itself stays in the foreground.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-read-efault
mkdir -p "$out"
rm -f "$out/host-echo.result" "$out/host-echo.log" "$out/host-echo.stdout"

# A failed previous run may still be waiting for the guest.
pkill -f "socket-read-efault/host-echo.py" 2>/dev/null || true

python3 tests/fixtures/socket-read-efault/host-echo.py \
    --host 127.0.0.1 \
    --port 23082 \
    --result "$out/host-echo.result" \
    --log "$out/host-echo.log" \
    --timeout 240 \
    > "$out/host-echo.stdout" 2>&1 &
