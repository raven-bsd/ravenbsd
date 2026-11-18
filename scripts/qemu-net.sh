#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S95 reproducible QEMU user-network gate. The release image is booted with an explicit
# virtio-net-pci device (vendor 0x1af4/device 0x1041, the devd driver), then the test logs in as
# root, runs ifconfig and pings the QEMU gateway 10.0.2.2. The same arguments are in
# tests/qtest/cases/network.toml; this script does not synthesize or fake a network.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) ;;
    *)
        echo "qemu-net: only x86_64 is supported, got $arch" >&2
        exit 2
        ;;
esac

printf '%s\n' \
    'qemu-net: args: -nic none -netdev user,id=n0 -device virtio-net-pci,netdev=n0,disable-legacy=on'

cargo xtask qtest --case network --arch "$arch"

log=build/qtest/network.log
[ -f "$log" ] || {
    echo "qemu-net: missing $log" >&2
    exit 1
}

grep -q '^netd: interface eth0 10\.0\.2\.15/24$' "$log"
grep -q '^ping: 10\.0\.2\.2 seq=1 rtt=' "$log"
grep -q '^ping: [1-9][0-9]* packets transmitted, [1-9][0-9]* received, 0% packet loss$' "$log"
if grep -q 'netd:.*nic:info.*no such file' "$log"; then
    echo "qemu-net: netd nic:info failure present" >&2
    exit 1
fi

printf 'qemu-net: pass (%s)\n' "$log"
