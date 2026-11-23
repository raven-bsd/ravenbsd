#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K435 K6: start the host peer before QEMU boots. QEMU forwards two ephemeral host ports to the
# fixed guest ports; the peer reads the assigned host ports from QEMU's monitor socket.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=${SSH_CONTRACT_OUT:-build/fixtures/ssh-contract}
mkdir -p "$out"
rm -f "$out/host.result" "$out/host.log" "$out/host.stdout" "$out/monitor.sock"
pkill -f "$root/tests/smp/ssh-contract/host_accept.py" 2>/dev/null || true
set -- --host 127.0.0.1 --monitor "$out/monitor.sock" \
    --result "$out/host.result" --log "$out/host.log" --timeout 600
if [ -n "${SSH_CONTRACT_K6_LOOP:-}" ]; then
    set -- "$@" --k6-loop "$SSH_CONTRACT_K6_LOOP"
fi
if [ "${SSH_CONTRACT_K6_READY:-0}" = "1" ]; then
    set -- "$@" --k6-ready
fi
if [ "${SSH_CONTRACT_K1K8:-0}" = "1" ]; then
    set -- "$@" --k1k8
fi
python3 "$root/tests/smp/ssh-contract/host_accept.py" "$@" \
    > "$out/host.stdout" 2>&1 &
