#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: run-qemu-aarch64.sh <BOOTAA64.EFI> <testkernel.elf> <initfs.img> [boot.toml]
# Builds a temporary ESP and boots it with AAVMF on QEMU virt. PSCI SYSTEM_OFF exits QEMU with
# status 0 for both success and failure, so the result is classified from the serial line:
# `TESTKERNEL OK` is success; `TESTKERNEL FAIL` or a missing OK line is failure.

set -u

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 <BOOTAA64.EFI> <testkernel.elf> <initfs.img> [boot.toml]" >&2
    exit 2
fi

loader=$1
kernel=$2
initfs=$3
boot_toml=${4:-}
memory=${QEMU_MEMORY:-512M}

if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
    echo "error: qemu-system-aarch64 not found; install QEMU aarch64 support" >&2
    exit 2
fi

code=${RAVEN_AAVMF_CODE:-/usr/share/AAVMF/AAVMF_CODE.fd}
vars_source=${RAVEN_AAVMF_VARS:-/usr/share/AAVMF/AAVMF_VARS.fd}
if [ ! -f "$code" ]; then
    echo "error: AAVMF code not found at $code (set RAVEN_AAVMF_CODE)" >&2
    exit 2
fi
if [ ! -f "$vars_source" ]; then
    echo "error: AAVMF vars not found at $vars_source (set RAVEN_AAVMF_VARS)" >&2
    exit 2
fi

esp=$(mktemp -d)
vars=$(mktemp)
log=$(mktemp)
trap 'rm -rf "$esp"; rm -f "$vars" "$log"' EXIT

mkdir -p "$esp/EFI/BOOT" "$esp/raven"
cp "$loader" "$esp/EFI/BOOT/BOOTAA64.EFI"
cp "$kernel" "$esp/raven/kernel.elf"
cp "$initfs" "$esp/raven/initfs.img"
if [ -n "$boot_toml" ]; then
    cp "$boot_toml" "$esp/raven/boot.toml"
fi
cp "$vars_source" "$vars"

timeout 30s qemu-system-aarch64 \
    -M virt -cpu cortex-a72 -m "$memory" \
    -nographic -monitor none -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "format=raw,file=fat:rw:$esp" \
    >"$log" 2>&1
status=$?
cat "$log"

if grep -q "TESTKERNEL OK" "$log"; then
    exit 0
fi
if grep -q "TESTKERNEL FAIL" "$log"; then
    echo "run-qemu-aarch64: TESTKERNEL FAIL (qemu status $status)" >&2
else
    echo "run-qemu-aarch64: no TESTKERNEL OK line (qemu status $status)" >&2
fi
exit 1
