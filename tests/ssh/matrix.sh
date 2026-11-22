#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 per-step SSH gate matrix (S0-S8) over the K194 tap. Brings the bridge up, builds the
# throw-away S0 echo initfs, boots the release SSH image once, and writes
# build/ssh/matrix/{matrix.tsv,attempts.tsv}. Needs passwordless sudo for the tap helper; refuses
# cleanly otherwise.
#
# Usage: sh tests/ssh/matrix.sh
# Outer acceptance timeout: 900 s; the inner commands have their own proportional caps.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "matrix: root required for the K194 tap helper; refusing" >&2
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

# S0 is a separate throw-away `server` initfs. A build failure is reported by the S0 row; it does
# not hide the SSH rows.
echo "matrix: building the S0 echo fixture" >&2
timeout 300 sh tests/fixtures/ssh-echo/build.sh || echo "matrix: S0 fixture build failed" >&2

$SUDO sh scripts/qemu-net.sh up
up=1
timeout 600 python3 tests/ssh/matrix.py
