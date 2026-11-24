#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K375: run the fake SSH peer tracer against the guest sshd on the K194
# tap/bridge. Test tooling only: it boots a prebuilt release image, waits for
# TCP 22, and writes byte-timestamped logs under build/fixtures/ssh-peer/.
#
# Prerequisites:
#   * the peer is built with the host cc (see docs/ssh-peer-tracer.md);
#   * a release image with the K195 sshd staging, e.g. built on the K195 branch
#     with `cargo xtask image --arch x86_64 --profile release
#     --output build/qtest/ssh-login.img`;
#   * passwordless sudo for the K194 tap helper (it refuses cleanly otherwise).
#
# Overrides: RAVEN_SSH_PEER=path to the peer binary (default
# /tmp/opencode/ssh-peer), RAVEN_SSH_PEER_IMAGE=path to the image (default
# build/qtest/ssh-login.img).
#
# Usage: sh tests/fixtures/ssh-peer/run.sh
# Outer acceptance timeout: 600 s.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

peer=${RAVEN_SSH_PEER:-/tmp/opencode/ssh-peer}
image=${RAVEN_SSH_PEER_IMAGE:-build/qtest/ssh-login.img}
out=build/fixtures/ssh-peer
tap=ravendemo0
guest=10.0.2.15
port=22

die() {
    echo "ssh-peer: $*" >&2
    exit 3
}

[ -x "$peer" ] || die "missing peer binary $peer; build it with cc (see docs/ssh-peer-tracer.md)"
[ -f "$image" ] || die "missing image $image; build it on the K195 branch (see docs/ssh-peer-tracer.md)"
command -v qemu-system-x86_64 >/dev/null 2>&1 || die "qemu-system-x86_64 is required"
command -v python3 >/dev/null 2>&1 || die "python3 is required for the bounded wait"

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
[ -f "$code" ] || code=/usr/share/OVMF/OVMF_CODE.fd
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$vars_src" ] || vars_src=/usr/share/OVMF/OVMF_VARS.fd
[ -f "$code" ] && [ -f "$vars_src" ] || die "OVMF firmware not found"

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || die "root required for the K194 tap helper (passwordless sudo unavailable); refusing"
    SUDO="sudo -n"
fi

mkdir -p "$out"
serial=$out/serial.log
qemu_log=$out/qemu.log
vars=$out/vars.fd
cp "$vars_src" "$vars"
qemu_pid=

cleanup() {
    if [ -n "$qemu_pid" ]; then
        kill -TERM "$qemu_pid" 2>/dev/null || true
        wait "$qemu_pid" 2>/dev/null || true
        qemu_pid=
    fi
    $SUDO sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

echo "ssh-peer: tap up"
$SUDO sh scripts/qemu-net.sh up >/dev/null
echo "ssh-peer: boot $image on $tap"
: >"$serial"
: >"$qemu_log"
qemu-system-x86_64 -machine q35 -m 512M -smp 4 \
    -serial "file:$serial" -display none -monitor none -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "format=raw,file=$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -nic none \
    -netdev "tap,id=n0,ifname=$tap,script=no,downscript=no" \
    -device virtio-net-pci,netdev=n0,disable-legacy=on \
    >>"$qemu_log" 2>&1 &
qemu_pid=$!

echo "ssh-peer: wait for $guest:$port"
timeout 180 python3 - "$guest" "$port" <<'PY' || die "guest TCP 22 never listened"
import socket, sys, time
host, port = sys.argv[1], int(sys.argv[2])
deadline = time.time() + 175
while time.time() < deadline:
    try:
        with socket.create_connection((host, port), timeout=1):
            sys.exit(0)
    except OSError:
        time.sleep(0.5)
sys.exit(1)
PY

echo "ssh-peer: banner only"
"$peer" --host "$guest" --port "$port" --mode client --idle-ms 3000 \
    --log "$out/guest-banner.log" >"$out/guest-banner.stdout" || true

echo "ssh-peer: banner + KEXINIT"
"$peer" --host "$guest" --port "$port" --mode client --send-banner --send-kexinit \
    --idle-ms 5000 --log "$out/guest-kexinit.log" >"$out/guest-kexinit.stdout" || true

echo "ssh-peer: banner + KEXINIT + KEX_ECDH_INIT"
"$peer" --host "$guest" --port "$port" --mode client --send-banner --send-kexinit \
    --send-kex-ecdh-init --idle-ms 5000 \
    --log "$out/guest-ecdh.log" >"$out/guest-ecdh.stdout" || true

echo "ssh-peer: logs in $out"
for log in guest-banner.log guest-kexinit.log guest-ecdh.log; do
    printf 'ssh-peer: %s\n' "$out/$log"
done
printf 'ssh-peer: done (tracer output only; the logs are the evidence)\n'
