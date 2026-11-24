#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot K231 host client before QEMU boots. It retries the hostfwd port until the
# guest listener accepts, then runs the exec'd-session exchange. The qtest itself stays in the
# foreground; the guest log carries the verdict.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/socket-inet-exec
mkdir -p "$out"
rm -f "$out/host-client.result" "$out/host-client.log" "$out/host-client.stdout"

# A failed previous run may still be waiting for the guest.
pkill -f "socket-inet-exec/host-client.py" 2>/dev/null || true

python3 tests/fixtures/socket-inet-exec/host-client.py \
    --host 127.0.0.1 \
    --port 23081 \
    --result "$out/host-client.result" \
    --log "$out/host-client.log" \
    --timeout 240 \
    > "$out/host-client.stdout" 2>&1 &
