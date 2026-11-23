#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K567 rev 336: bring up the K194 tap and run the timestamped first-byte probe.
# The probe itself boots QEMU; the tap helper is the only host setup. The wrapper
# is meant to be run under scripts/build-slot.sh because it starts a QEMU.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "ssh-first-byte: root required for the K194 tap helper" >&2
        exit 3
    }
    SUDO="sudo -n"
fi

up=0
down() {
    if [ "$up" -eq 1 ]; then
        $SUDO sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
        up=0
    fi
}
trap down EXIT HUP INT TERM

$SUDO sh scripts/qemu-net.sh up
up=1
mkdir -p build/ssh/first-byte
{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uptime
    printf 'qemu=%s\n' "$(pgrep -c -x qemu-system-x86_64 2>/dev/null || true)"
} >> build/ssh/first-byte/host.txt
python3 tests/fixtures/ssh-first-byte/probe.py
rc=$?
exit "$rc"
