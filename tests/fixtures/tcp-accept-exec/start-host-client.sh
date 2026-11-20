#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Start the K225 host client before QEMU boots; it retries the hostfwd port.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/tcp-accept-exec
mkdir -p "$out"
rm -f "$out/host-client.result" "$out/host-client.log"

pkill -f "tcp-accept-exec/host_client.py" 2>/dev/null || true

python3 tests/fixtures/tcp-accept-exec/host_client.py \
    --result "$out/host-client.result" \
    --log "$out/host-client.log" \
    --timeout 240 \
    > "$out/host-client.stdout" 2>&1 &
