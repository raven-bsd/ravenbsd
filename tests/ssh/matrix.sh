#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 per-step SSH gate matrix (S0-S8) over the K194 tap. Runs the K430 pty-alloc qtest, builds
# the throw-away S0 echo initfs, boots the release SSH image once, and writes
# build/ssh/matrix/{matrix.tsv,attempts.tsv,pty-alloc.*}. Needs passwordless sudo for the tap
# helper; refuses cleanly otherwise.
#
# Usage: sh tests/ssh/matrix.sh
# Outer acceptance timeout: 900 s; the inner commands have their own proportional caps.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

bridge=${RAVEN_QEMU_BRIDGE:-ravenbr0}
tap=${RAVEN_QEMU_TAP:-ravendemo0}
net() {
    $SUDO env RAVEN_QEMU_BRIDGE="$bridge" RAVEN_QEMU_TAP="$tap" sh scripts/qemu-net.sh "$@"
}

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
        net down >/dev/null 2>&1 || true
        up=0
    fi
}
trap down EXIT HUP INT TERM

# S8's dynamic pty case comes from merged K430. Its measured run after K442 is ~39 s; 75 s is a
# small margin.
mkdir -p build/ssh/matrix
echo "matrix: running the K430 pty-alloc qtest" >&2
pty_exit=0
timeout 75 cargo xtask qtest --case pty-alloc --arch x86_64 \
    > build/ssh/matrix/pty-alloc.log 2>&1 || pty_exit=$?
echo "$pty_exit" > build/ssh/matrix/pty-alloc.exit

# S0 is a separate throw-away `server` initfs. A build failure is reported by the S0 row; it does
# not hide the SSH rows.
echo "matrix: building the S0 echo fixture" >&2
timeout 120 sh tests/fixtures/ssh-echo/build.sh || echo "matrix: S0 fixture build failed" >&2

net down >/dev/null 2>&1 || true
net up
up=1
RAVEN_QEMU_TAP="$tap" # matrix.py measured: the 600 s cap expired during S7 on rev 25; the row set needs ~550-600 s,
# so 780 s is the measured runtime plus margin. The outer acceptance cap is 900 s.
timeout 780 python3 tests/ssh/matrix.py
