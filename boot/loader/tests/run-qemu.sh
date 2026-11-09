#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: run-qemu.sh <loader.efi> <kernel.elf> <initfs.img> [boot.toml]
# Builds a temporary ESP, runs OVMF + QEMU with a 20 second timeout, and forwards QEMU's exit
# status (33 = success from the C5 test-exit protocol). QEMU_MEMORY overrides the default 512M.

set -u

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 <loader.efi> <kernel.elf> <initfs.img> [boot.toml]" >&2
    exit 2
fi

loader=$1
kernel=$2
initfs=$3
boot_toml=${4:-}
memory=${QEMU_MEMORY:-512M}

esp=$(mktemp -d)
vars=$(mktemp)
trap 'rm -rf "$esp"; rm -f "$vars"' EXIT

mkdir -p "$esp/EFI/BOOT" "$esp/raven"
cp "$loader" "$esp/EFI/BOOT/BOOTX64.EFI"
cp "$kernel" "$esp/raven/kernel.elf"
cp "$initfs" "$esp/raven/initfs.img"
if [ -n "$boot_toml" ]; then
    cp "$boot_toml" "$esp/raven/boot.toml"
fi
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$vars"

timeout 20s qemu-system-x86_64 \
    -machine q35 -m "$memory" \
    -serial stdio -display none -monitor none -no-reboot \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$vars" \
    -drive "format=raw,file=fat:rw:$esp" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04
status=$?
exit "$status"
