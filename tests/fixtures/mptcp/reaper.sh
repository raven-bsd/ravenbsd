#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# MPTCP2c fixture reaper: wait for the case's QEMU instance, then remove the two taps. The qtest
# harness has no post hook, so the cleanup lives here. It is idempotent with the helper's `down`
# and with host.py's own failure path.

set -eu

root=${1:-}
[ -n "$root" ] || {
    echo "usage: reaper.sh <repository-root>" >&2
    exit 2
}
cd "$root"

qmp=$root/build/qtest/mptcp-qmp.sock
out=build/fixtures/mptcp
mkdir -p "$out"

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    if ! sudo -n true 2>/dev/null; then
        echo "reaper: root required (passwordless sudo unavailable); refusing" >&2
        exit 3
    fi
    SUDO="sudo -n"
fi

# The QMP socket appears when QEMU starts; the image build may run for a few minutes first.
attempts=0
while [ ! -S "$qmp" ] && [ "$attempts" -lt 4500 ]; do
    sleep 0.2
    attempts=$((attempts + 1))
done
if [ -S "$qmp" ]; then
    echo "reaper: qmp up"
else
    echo "reaper: qmp did not appear; removing any partial fixture"
fi

# A crashed QEMU can leave the socket file behind, so also watch the process that owns the tap.
attempts=0
while [ -S "$qmp" ] && [ "$attempts" -lt 4500 ]; do
    if ! pgrep -f "ifname=mptcp-tap0" >/dev/null 2>&1; then
        break
    fi
    sleep 0.2
    attempts=$((attempts + 1))
done

$SUDO sh scripts/qtest-net-two-taps.sh down || true
echo "reaper: taps down"
