#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K431 S5 private tap wrapper. The fixture uses 10.43.0.0/24 on k431br0/k431tap0 so it never shares
# the K194 10.0.2.15/ravendemo0 network with another lane. The guest has a static 10.43.0.15.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

bridge=k431br0
tap=k431tap0
if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    SUDO="sudo -n"
    $SUDO true 2>/dev/null || {
        echo "socket-write-identity: passwordless sudo required for the private tap" >&2
        exit 2
    }
fi

cleanup() {
    timeout 20 $SUDO ip link del "$tap" >/dev/null 2>&1 || true
    timeout 20 $SUDO ip link del "$bridge" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

cleanup
timeout 30 $SUDO ip link add "$bridge" type bridge
timeout 30 $SUDO ip addr add 10.43.0.2/24 dev "$bridge"
timeout 30 $SUDO ip link set "$bridge" up
timeout 30 $SUDO ip tuntap add dev "$tap" mode tap user "${SUDO_USER:-$(id -un)}"
timeout 30 $SUDO ip link set "$tap" master "$bridge"
timeout 30 $SUDO ip link set "$tap" up

timeout 210 cargo xtask qtest --case socket-write-identity-tap --arch x86_64
