#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K195 host-to-guest SSH gate over the K194 tap/bridge. Run from the repository
# root after `cargo xtask image --arch x86_64 --profile release --output
# build/qtest/ssh-login.img`:
#
#   sh tests/ssh/ssh-login.sh
#
# The helper brings up the bridge/tap as root with passwordless sudo, then runs
# the whole gate in one command: a raw byte probe with the K375 fake peer
# tracer, ten `ssh raven@<guest> ls` command sessions (RAVEN_SSH_REPEAT), the
# wrong-password refusal and the `ssh -tt` PTY case. A failure is classified and
# quoted with its evidence: a = banner not received, b = TX wedge (0 bytes),
# c = auth refused, d = session/command failure. Exit codes: 0 pass, 2 setup,
# 10/11/12/13 for a/b/c/d. The bridge is always torn down.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

image=${RAVEN_SSH_IMAGE:-build/qtest/ssh-login.img}
[ -f "$image" ] || {
    echo "ssh-login: missing $image; build it with" >&2
    echo "ssh-login: cargo xtask image --arch x86_64 --profile release --output $image" >&2
    exit 3
}

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "ssh-login: root required for the K194 tap helper (passwordless sudo unavailable); refusing" >&2
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
RAVEN_SSH_IMAGE="$image" python3 tests/ssh/ssh-login.py
