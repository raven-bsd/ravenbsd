#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K371 default-profile tap proof: boot the release image on the K194 tap, prove
# netd reached ready with no pf ruleset, check host/guest ping, and copy the
# serial/up/down/DNS logs into tests/evidence/network/postpf/default. A missing
# image fails before QEMU with the exact build command (the test-first red).

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

image=${RAVEN_QEMU_IMAGE:-build/qtest/net-postpf.img}
if [ ! -f "$image" ]; then
    printf 'net-postpf: missing release image %s; run cargo xtask image --arch x86_64 --profile release --output %s\n' \
        "$image" "$image" >&2
    exit 1
fi

out=tests/evidence/network/postpf/default
mkdir -p "$out"

RAVEN_QEMU_IMAGE="$image" sh tests/scripts/qemu-net-selftest.sh

serial=build/qemu-net/serial.log
[ -f "$serial" ] || {
    echo "net-postpf: missing $serial" >&2
    exit 1
}
grep -q '^netd: pf absent' "$serial" || {
    echo "net-postpf: default release did not log netd: pf absent" >&2
    exit 1
}
grep -q '^netd: interface eth0 10\.0\.2\.15/24' "$serial" || {
    echo "net-postpf: default release did not configure eth0" >&2
    exit 1
}
grep -q '^netd: ready' "$serial" || {
    echo "net-postpf: default release did not reach netd: ready" >&2
    exit 1
}
grep -q '0% packet loss' "$serial" || {
    echo "net-postpf: serial has no successful host/guest ping" >&2
    exit 1
}
if grep -q '^netd: pf loaded' "$serial"; then
    echo "net-postpf: default release unexpectedly loaded a pf ruleset" >&2
    exit 1
fi

cp "$serial" "$out/serial.log"
cp build/qemu-net/up.txt "$out/up.txt"
cp build/qemu-net/down.txt "$out/down.txt"
cp build/qemu-net/dns.txt "$out/dns.txt"

echo "net-postpf: default-release tap proof ok (pf absent, netd ready, host/guest ping)"
