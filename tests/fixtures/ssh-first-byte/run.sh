#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K567 rev 344: build the release image the row boots, then run the timestamped
# first-byte probe against that exact image. The probe itself boots QEMU; the tap
# helper is the only host setup. The wrapper is meant to be run under
# scripts/build-slot.sh because it builds an image and starts a QEMU.
#
# The image is stamped with the tree revision. Reusing a stale or foreign image is
# what made the case-wrapper console smoke wait for a password prompt the image
# never printed (Stuart's no-password ssh-login.img); rebuild at HEAD instead and
# let the probe and the qtest case boot the same artifact.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

image=build/qtest/ssh-first-byte.img
stamp=$image.head
commit=$(git rev-parse HEAD)

mkdir -p build/qtest
if command -v flock >/dev/null 2>&1; then
    exec 9>"build/qtest/.ssh-first-byte.lock"
    flock -w 900 9 || {
        echo "ssh-first-byte: image build lock timeout" >&2
        exit 1
    }
fi

if [ ! -f "$image" ] || [ ! -f "$stamp" ] ||
    [ "$(cat "$stamp" 2>/dev/null)" != "$commit" ]; then
    printf 'ssh-first-byte: building %s at %s\n' "$image" "$commit"
    cargo xtask image --arch x86_64 --profile release \
        --cmdline 'console=ttyS0 test-exit=1' --output "$image"
    printf '%s\n' "$commit" >"$stamp"
fi

if command -v flock >/dev/null 2>&1; then
    exec 9>&-
fi

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

tap=${RAVEN_QEMU_TAP:-ravendemo0}
$SUDO sh scripts/qemu-net.sh up
up=1
# qemu-net.sh trusts /run/raven-qemu-net.state; a stale state file returns the
# contract without creating the interface, and QEMU then dies with an empty
# serial.  Verify the tap itself and repair the stale state once, deterministically.
if ! ip link show "$tap" >/dev/null 2>&1; then
    printf 'ssh-first-byte: tap %s missing after qemu-net.sh up; repairing stale state\n' "$tap" >&2
    $SUDO sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
    $SUDO sh scripts/qemu-net.sh up
fi
if ! ip link show "$tap" >/dev/null 2>&1; then
    printf 'ssh-first-byte: tap %s is not up; refusing to boot QEMU (no empty serial)\n' "$tap" >&2
    exit 4
fi
mkdir -p build/ssh/first-byte
{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uptime
    printf 'qemu=%s\n' "$(pgrep -c -x qemu-system-x86_64 2>/dev/null || true)"
    printf 'tap=%s\n' "$tap"
} >> build/ssh/first-byte/host.txt
RAVEN_QEMU_TAP="$tap" RAVEN_SSH_IMAGE="$image" python3 tests/fixtures/ssh-first-byte/probe.py
rc=$?
exit "$rc"
