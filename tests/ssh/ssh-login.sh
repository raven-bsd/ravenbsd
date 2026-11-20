#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-to-guest SSH test over the K194 tap helper. Run from the repository root:
#   sh tests/ssh/ssh-login.sh
# The helper needs passwordless sudo; it refuses cleanly otherwise.

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
python3 tests/ssh/ssh-login.py
