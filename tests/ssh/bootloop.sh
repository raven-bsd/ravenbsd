#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K446 multi-boot banner/auth stability: 5 fresh boots x 5 fresh connections.
#
# Prerequisites:
#   cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img
#   (built from the clean checkout at HEAD; RAVEN_SSH_IMAGE overrides the path)
#
# Run from the repository root; the K194 tap/bridge needs root (passwordless sudo):
#
#   timeout 1800 sh tests/ssh/bootloop.sh
#
# Every attempt records the raw banner bytes (zero bytes is a FAIL with its
# boot/attempt number), the exact banner line, the client's
# `Authenticated ... using "password"` line, exit code and wall time. Evidence is
# written under build/ssh-bootloop/<UTC>/ (or $RAVEN_SSH_EVIDENCE).

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

# One tap name for both the K194 helper and the QEMU boots: the tap must be
# created for this user (qemu-net.sh uses $SUDO_USER) or QEMU cannot open it.
RAVEN_QEMU_TAP=${RAVEN_QEMU_TAP:-ravendemo0}
export RAVEN_QEMU_TAP

image=${RAVEN_SSH_IMAGE:-build/qtest/ssh-login.img}
[ -f "$image" ] || {
    echo "bootloop: missing $image; build it with" >&2
    echo "bootloop: cargo xtask image --arch x86_64 --profile release --output $image" >&2
    exit 3
}

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "bootloop: root required for the K194 tap helper (passwordless sudo unavailable); refusing" >&2
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
RAVEN_BOOTLOOP_SUDO=$([ -n "$SUDO" ] && echo 1 || echo 0) \
    RAVEN_SSH_IMAGE="$image" python3 tests/ssh/bootloop.py
