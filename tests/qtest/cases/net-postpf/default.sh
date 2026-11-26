#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K371 default-profile tap proof. Bring up the K194 tap with the first-party
# helper, boot the release image at build/qtest/net-postpf.img, prove netd is
# ready with no pf ruleset, prove host/guest/internet ping, and stage the serial,
# up and down logs in build/qemu-net-postpf/default. The durable ticket evidence
# is copied from that build staging directory after the qtest run; the pre must
# not dirty the release image's clean-worktree build. A missing image fails
# before QEMU with the exact build command (the test-first red).

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

image=${RAVEN_QEMU_IMAGE:-build/qtest/net-postpf.img}
if [ ! -f "$image" ]; then
    printf 'net-postpf: missing release image %s; run cargo xtask image --arch x86_64 --profile release --output %s\n' \
        "$image" "$image" >&2
    exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    if ! sudo -n true 2>/dev/null; then
        echo "net-postpf: root required for the K194 tap (passwordless sudo unavailable)" >&2
        exit 3
    fi
    SUDO="sudo -n"
fi

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
[ -f "$code" ] || code=/usr/share/OVMF/OVMF_CODE.fd
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$vars_src" ] || vars_src=/usr/share/OVMF/OVMF_VARS.fd
if [ ! -f "$code" ] || [ ! -f "$vars_src" ]; then
    echo "net-postpf: OVMF firmware not found" >&2
    exit 3
fi

out=build/qemu-net-postpf/default
mkdir -p "$out"
up_log=$out/up.txt
down_log=$out/down.txt
serial=$out/serial.log
vars=$(mktemp "${TMPDIR:-/tmp}/raven-k371-ovmf.XXXXXX")
cp "$vars_src" "$vars"
tap=ravendemo0
net_up=0

cleanup() {
    if [ "$net_up" -eq 1 ]; then
        $SUDO sh scripts/qemu-net.sh down >"$down_log" 2>&1 || true
    fi
    rm -f "$vars"
}
trap cleanup EXIT HUP INT TERM

$SUDO sh scripts/qemu-net.sh up | tee "$up_log"
net_up=1
tap=$(sed -n 's/^tap=\([^ ]*\).*/\1/p' "$up_log" | head -n 1)
[ -n "$tap" ] || {
    echo "net-postpf: qemu-net up did not print a tap device" >&2
    exit 1
}

status=0
timeout 180 python3 tests/qtest/cases/net-postpf/tap.py \
    "$image" "$tap" "$serial" "$code" "$vars" || status=$?

$SUDO sh scripts/qemu-net.sh down | tee "$down_log"
net_up=0

if [ "$status" -ne 0 ]; then
    echo "net-postpf: tap proof failed (see $serial)" >&2
    tail -n 40 "$serial" >&2 || true
    exit 1
fi

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

echo "net-postpf: default-release tap proof ok (pf absent, netd ready, host/guest ping)"
