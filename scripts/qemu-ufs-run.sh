#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K193 bootable one-disk UFS demo. The qcow2 contains the FAT ESP first and the
# UFS2 root second; it is attached as exactly one virtio-blk drive. The default
# network is the K194 tap/bridge helper; --user-net is the documented fallback.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root=$(CDPATH= cd -- "$here/../../.." && pwd -P)
qcow2=$here/ravenbsd-ufs-demo.qcow2
code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
vars=$here/OVMF_VARS.fd
helper=$root/scripts/qemu-net.sh
user_net=0
tap=${RAVEN_QEMU_TAP:-ravendemo0}
net_up=0
guest_ip=10.0.2.15
demo_user=raven
demo_password=${RAVEN_DEMO_PASSWORD:-raven}

usage() {
    cat <<'EOF'
usage:
  run.sh             boot the one-disk UFS demo with the bidirectional K194 tap
  run.sh --user-net  boot with QEMU user networking (non-bidirectional fallback)
  run.sh --check     verify qemu-img, OVMF and the network helper; do not boot
  run.sh --help
EOF
}

fail() {
    echo "run.sh: $*" >&2
    exit 1
}

refuse_net() {
    echo "run.sh: $*" >&2
    echo "run.sh: rerun with --user-net for the non-bidirectional fallback" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --user-net) user_net=1; shift ;;
        --check) check_only=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
check_only=${check_only:-0}

print_demo() {
    printf 'run.sh: guest=%s user=%s password=%s\n' "$guest_ip" "$demo_user" "$demo_password"
    printf 'run.sh: ssh raven@%s ls   # requires K195 sshd + K225 TCP; pending\n' "$guest_ip"
}

check_tools() {
    command -v qemu-img >/dev/null 2>&1 || fail "qemu-img not found (install qemu-utils)"
    command -v qemu-system-x86_64 >/dev/null 2>&1 ||
        fail "qemu-system-x86_64 not found"
    [ -f "$qcow2" ] || fail "missing $qcow2"
    [ -f "$code" ] || fail "OVMF code not found: $code"
    [ -f "$vars_src" ] || fail "OVMF vars not found: $vars_src"
    [ -f "$helper" ] || fail "network helper not found: $helper"
    sh -n "$helper" || fail "network helper is not a shell script: $helper"
    qemu-img info "$qcow2" >/dev/null 2>&1 || fail "qemu-img cannot read $qcow2"
}

check_tools

if [ "$check_only" -eq 1 ]; then
    print_demo
    if grep -q '^ *up)' "$helper"; then
        echo "run.sh: check PASS (qemu-img, OVMF, bidirectional helper)"
    else
        echo "run.sh: check PASS (qemu-img, OVMF, helper; helper has no up/down yet)"
    fi
    exit 0
fi

cp "$vars_src" "$vars"
cleanup() {
    if [ "$net_up" -eq 1 ] && [ -f "$helper" ]; then
        sh "$helper" down >/dev/null 2>&1 || true
    fi
    rm -f "$vars"
}
trap cleanup EXIT HUP INT TERM

net_args=
if [ "$user_net" -eq 1 ]; then
    net_args="-netdev user,id=n0 -device virtio-net-pci,netdev=n0,disable-legacy=on"
else
    grep -q '^ *up)' "$helper" ||
        refuse_net "$helper has no up/down; K194 tap/bridge is required for bidirectional networking"
    contract=$(sh "$helper" up) || refuse_net "cannot bring up the tap/bridge (root/tap required)"
    tap=$(printf '%s\n' "$contract" | sed -n 's/^tap=\([^ ]*\).*/\1/p' | head -n 1)
    [ -n "$tap" ] || refuse_net "network helper did not report a tap device"
    net_up=1
    net_args="-netdev tap,id=n0,ifname=$tap,script=no,downscript=no -device virtio-net-pci,netdev=n0,disable-legacy=on"
    guest_ip=$(printf '%s\n' "$contract" | sed -n 's/.* guest=\([^ ]*\).*/\1/p' | head -n 1)
    [ -n "$guest_ip" ] || guest_ip=10.0.2.15
fi

print_demo

if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    accel="-enable-kvm -cpu host"
else
    accel="-cpu max"
fi

qemu-system-x86_64 \
    -machine q35 \
    -m 4096 \
    -smp 4 \
    $accel \
    -serial stdio \
    -display none \
    -monitor none \
    -nic none \
    -global virtio-blk-pci.disable-legacy=on \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "file=$qcow2,if=virtio,format=qcow2" \
    $net_args
