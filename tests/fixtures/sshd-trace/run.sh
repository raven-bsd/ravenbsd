#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498: run the release sshd with -ddd over a private QEMU user-net host forward,
# make one password login and one command, then keep the QEMU serial log. The
# release image and the upstream sshd binary are unchanged; the fixture overlay
# only adds the debug service flags/config. The trace QEMU is isolated on port
# 2498; the shared Ken matrix tap/bridge is not used.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

sh tests/fixtures/sshd-trace/build.sh

out=build/fixtures/sshd-trace
image=$out/sshd-trace.img
if [ ! -f "$image" ]; then
    cargo xtask image --arch x86_64 --profile release \
        --initfs "$out/initfs.img" --output "$image"
fi

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$code" ] || { echo "sshd-trace: missing OVMF code $code" >&2; exit 2; }
[ -f "$vars_src" ] || { echo "sshd-trace: missing OVMF vars $vars_src" >&2; exit 2; }
vars=$out/vars.fd
serial=$out/serial.log
client=$out/client.log
cp "$vars_src" "$vars"
rm -f "$serial" "$client"

qemu-system-x86_64 \
    -machine q35 -m 512M -smp 4 \
    -serial "file:$serial" -display none -monitor none -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "format=raw,file=$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -nic none \
    -netdev user,id=n0,hostfwd=tcp:127.0.0.1:2498-:22 \
    -device virtio-net-pci,netdev=n0,disable-legacy=on &
qemu_pid=$!
cleanup() {
    kill "$qemu_pid" 2>/dev/null || true
    wait "$qemu_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

ready=0
index=0
while [ "$index" -lt 120 ]; do
    if [ -f "$serial" ] && grep -q 'svcd: sshd up' "$serial"; then
        ready=1
        break
    fi
    sleep 0.5
    index=$((index + 1))
done
if [ "$ready" -ne 1 ]; then
    echo "sshd-trace: sshd did not start; serial=$serial" >&2
    exit 1
fi

sleep 1
SSHD_TRACE_CLIENT_LOG=$client python3 tests/fixtures/sshd-trace/client.py

# Give the server loop time to write its post-command lines, then keep the log.
sleep 2
cleanup
trap - EXIT INT TERM
echo "sshd-trace: serial=$serial"
