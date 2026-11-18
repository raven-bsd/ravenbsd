#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the S85c SMP IPC stress fixture. Usage:
#   run.sh tcg 4      # TCG, four CPUs (default)
#   run.sh kvm 2      # KVM, two CPUs
#   run.sh kvm 4      # KVM, four CPUs
#
# The fixture itself runs 50 rounds of channel/ring/trace traffic. The script rejects a boot that
# does not print the final "smp-ipc: done" marker; it never treats a QEMU exit code alone as a
# pass.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-tcg}
smp=${2:-4}
case "$mode" in
    tcg) accel= ;;
    kvm) accel="-enable-kvm -cpu host" ;;
    *) echo "usage: $0 [tcg|kvm] [2|4]" >&2; exit 2 ;;
esac
case "$smp" in
    2|4) ;;
    *) echo "usage: $0 [tcg|kvm] [2|4]" >&2; exit 2 ;;
esac

out=build/smp/ipc
mkdir -p "$out"
sh "$root/tests/smp/ipc/build.sh"

image=$out/image-$mode-$smp.img
log=$out/$mode-$smp.log
vars=$out/ovmf-vars-$mode-$smp.fd
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$vars"

cargo xtask image --arch x86_64 --profile base \
    --cmdline "console=ttyS0 test-exit=1 smp_user=on" \
    --initfs "$out/initfs.img" --output "$image" >/dev/null

set +e
# shellcheck disable=SC2086
timeout 180 qemu-system-x86_64 -machine q35 -m 512M -smp "$smp" $accel \
    -serial stdio -display none -monitor none -no-reboot \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$vars" \
    -drive format=raw,file="$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    > "$log" 2>&1
rc=$?
set -e

cat "$log"
if grep -q 'smp-ipc: done' "$log" && grep -q 'init exited (0)' "$log"; then
    echo "smp-ipc: $mode smp=$smp PASS (qemu $rc)"
    exit 0
fi
echo "smp-ipc: $mode smp=$smp FAIL (qemu $rc)" >&2
exit 1
