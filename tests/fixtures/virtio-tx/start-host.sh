#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K382 start the host peer before QEMU boots. A stale host.py from a previous run must not steal
# the one accepted connection; kill it and clear the per-run artifacts first.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/virtio-tx
pkill -f "tests/fixtures/virtio-tx/host.py" 2>/dev/null || true
rm -f "$out/host.log" "$out/host.result" "$out/host.stdout" "$out/qemu.pcap"
python3 tests/fixtures/virtio-tx/host.py > "$out/host.stdout" 2>&1 &
