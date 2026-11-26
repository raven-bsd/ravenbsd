#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S200 acceptance loop: boot the x86_64 base image fifty times with `-smp 4` and fail on the
# first boot that prints `smp: cpu N did not start`, reports fewer than four CPUs, or does not
# exit through the guest-success port. The QEMU arguments mirror tests/qtest/src/qemu.rs.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/qtest
loop=$out/smp-ap-bringup
image=$out/smp-ap-bringup.img
code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
qemu=${QEMU_X86_64:-qemu-system-x86_64}
boots=50

mkdir -p "$loop"

cargo xtask image --arch x86_64 --profile base \
    --cmdline "console=ttyS0 test-exit=1" --output "$image" \
    > "$loop/image.log" 2>&1

tsv=$loop/boots.tsv
printf 'boot\tcpus\tdid_not_start\texit\n' > "$tsv"

i=1
while [ "$i" -le "$boots" ]; do
    cp "$vars" "$loop/vars.fd"
    if timeout 120 "$qemu" -machine q35 -m 512M -smp 4 \
        -serial stdio -display none -monitor none -no-reboot \
        -drive if=pflash,format=raw,readonly=on,file="$code" \
        -drive if=pflash,format=raw,file="$loop/vars.fd" \
        -drive format=raw,file="$image" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
        > "$loop/boot-$i.log" 2>&1; then
        rc=0
    else
        rc=$?
    fi
    tr -d '\r' < "$loop/boot-$i.log" > "$loop/boot-$i.txt"
    cpus=$(sed -n 's/^smp: \([0-9][0-9]*\) cpus online$/\1/p' "$loop/boot-$i.txt" | tail -1)
    [ -n "$cpus" ] || cpus=0
    miss=0
    grep -q '^smp: cpu [0-9][0-9]* did not start$' "$loop/boot-$i.txt" && miss=1
    printf '%s\t%s\t%s\t%s\n' "$i" "$cpus" "$miss" "$rc" >> "$tsv"
    if [ "$cpus" -ne 4 ] || [ "$miss" -ne 0 ] || [ "$rc" -ne 33 ]; then
        echo "smp-ap-bringup: FAIL boot=$i cpus=$cpus did_not_start=$miss exit=$rc log=$loop/boot-$i.log" >&2
        exit 1
    fi
    i=$((i + 1))
done

echo "smp-ap-bringup: $boots/$boots boots clean"
