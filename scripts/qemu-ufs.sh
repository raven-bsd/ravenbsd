#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K176: boot the QEMU UFS-root preflight. The ESP image carries the loader, kernel and
# bootstrap initfs; the virtio-blk disk is the release UFS2 root. `make qemu-ufs` is
# this script with no flags.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

arch=x86_64
image=build/$arch/qemu-ufs.img
disk=build/$arch/qemu-ufs-root.img
vars=build/qemu-ufs.vars.fd

# K193: the bootable UFS root carries the same static disk tools as the S99 demo.
if [ "${QEMU_UFS_SKIP_TOOLS:-0}" != 1 ]; then
    sh scripts/ufs-demo.sh --prepare-tools
fi

cargo xtask image --arch "$arch" --profile qemu-ufs --output "$image"
test -f "$disk" || { echo "qemu-ufs: missing root disk $disk" >&2; exit 1; }

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
test -f "$code" || { echo "qemu-ufs: OVMF code not found: $code" >&2; exit 1; }
test -f "$vars_src" || { echo "qemu-ufs: OVMF vars not found: $vars_src" >&2; exit 1; }
cp "$vars_src" "$vars"

exec qemu-system-x86_64 \
    -machine q35 \
    -m 4096 \
    -smp 2 \
    -serial stdio \
    -display none \
    -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "format=raw,file=$image" \
    -drive "if=none,id=ufs,format=raw,file=$disk" \
    -device virtio-blk-pci,drive=ufs,disable-legacy=on \
    -nic none \
    -netdev user,id=n0 \
    -device virtio-net-pci,netdev=n0,disable-legacy=on
