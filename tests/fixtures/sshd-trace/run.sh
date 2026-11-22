#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K547: build the DEBUG3 overlay image, bring up an isolated private tap/bridge
# (static 10.0.2.15/24, no hostfwd: the slirp proxy is the known FIN/RST
# failure), run the host tap client and the guest loopback control in one boot,
# and tear the bridge down. The fixture owns the ravenbrtrace/ravetrace0 names;
# it does not touch Ken's tests/ssh matrix or the shared ravenbr0/ravendemo0.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

bridge=${RAVEN_TRACE_BRIDGE:-ravenbrtrace}
tap=${RAVEN_TRACE_TAP:-ravetrace0}

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "sshd-trace: root required for the tap helper (passwordless sudo unavailable); refusing" >&2
        exit 3
    }
    SUDO="sudo -n"
fi

down() {
    $SUDO ip link del "$tap" 2>/dev/null || true
    $SUDO ip link del "$bridge" 2>/dev/null || true
}
trap down EXIT HUP INT TERM

sh tests/fixtures/sshd-trace/build.sh

down
$SUDO ip link add name "$bridge" type bridge
$SUDO ip addr add 10.0.2.2/24 dev "$bridge"
$SUDO ip link set "$bridge" up
$SUDO ip tuntap add dev "$tap" mode tap user "$(id -un)"
$SUDO ip link set "$tap" master "$bridge"
$SUDO ip link set "$tap" up

RAVEN_TRACE_TAP="$tap" python3 tests/fixtures/sshd-trace/trace-host.py
