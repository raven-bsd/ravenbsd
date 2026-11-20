#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Final K195 tap gate. Brings up the K194 tap/bridge, runs the exact host SSH command with a
# 30 s ConnectTimeout and the wrong-password refusal, then tears the bridge down. Needs
# passwordless sudo; refuses cleanly otherwise.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

up=0
down() {
    if [ "$up" -eq 1 ]; then
        sudo -n sh scripts/qemu-net.sh down || true
        up=0
    fi
}
trap down EXIT INT TERM

sudo -n sh scripts/qemu-net.sh up
up=1
python3 tests/ssh/ssh-tap-gate.py
