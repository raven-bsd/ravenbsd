#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot K438 TCP host client before QEMU boots. It retries the hostfwd port until
# the guest listener accepts, verifies the exact payload and sends the acknowledgement. The
# qtest itself stays in the foreground.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/fdpass-tcp
mkdir -p "$out"
rm -f "$out/host.result" "$out/host.log" "$out/host.stdout"

pkill -f "fdpass-tcp/host.py" 2>/dev/null || true

python3 tests/qtest/cases/fdpass-tcp/host.py \
    --host 127.0.0.1 \
    --port 23084 \
    --result "$out/host.result" \
    --log "$out/host.log" \
    --timeout 240 \
    > "$out/host.stdout" 2>&1 &
