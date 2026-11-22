#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the one-shot K438 host peer before QEMU boots. It retries the hostfwd port until the
# guest accepts, reads the payload written through the received descriptor and answers. The
# qtest itself stays in the foreground.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=$root/build/fixtures/fdpass
mkdir -p "$out"
rm -f "$out/host.result" "$out/host.log" "$out/host.stdout"

pkill -f "fdpass-fixture/host-peer.py" 2>/dev/null || true

python3 tests/qtest/cases/fdpass-fixture/host-peer.py \
    --host 127.0.0.1 \
    --port 23082 \
    --result "$out/host.result" \
    --log "$out/host.log" \
    --timeout 240 \
    > "$out/host.stdout" 2>&1 &
