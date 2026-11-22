#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K435 K6: start the host peer before QEMU boots. It retries the hostfwd port until the guest
# listener accepts both connections.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/ssh-contract
mkdir -p "$out"
rm -f "$out/host.result" "$out/host.log" "$out/host.stdout"
pkill -f "ssh-contract/host_accept.py" 2>/dev/null || true
python3 tests/smp/ssh-contract/host_accept.py \
    --host 127.0.0.1 \
    --port 23197 \
    --result "$out/host.result" \
    --log "$out/host.log" \
    --timeout 120 \
    > "$out/host.stdout" 2>&1 &
