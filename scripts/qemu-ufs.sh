#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K138 interactive UFS QEMU demo. Boots the full ufs-demo image with the host-formatted UFS2
# GPT disk attached, keeps the serial console interactive and records an optional transcript.
# This is a wrapper over the same image/disk/device args as the ufs-disk qtest; it does not
# replace or weaken the qtest.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) ;;
    *)
        echo "qemu-ufs: only x86_64 is supported, got $arch" >&2
        exit 2
        ;;
esac

out=build/ufs-demo
image=${UFS_DEMO_IMAGE:-$out/ufs-demo.img}
disk=build/fixtures/ufs-demo/disk.img
log=${UFS_DEMO_LOG:-$out/qemu.log}
stamp=$out/image.ref
rebuild=${UFS_DEMO_REBUILD:-0}

say() { printf 'qemu-ufs: %s\n' "$*"; }
die() { printf 'qemu-ufs: %s\n' "$*" >&2; exit 2; }

mkdir -p "$out" "$(dirname "$log")"

if [ "$rebuild" = 1 ] || [ ! -f "$disk" ]; then
    say "preparing the UFS2 fixture disk"
    sh scripts/ufs-demo.sh --prepare
else
    sh scripts/ufs-demo.sh --prepare-tools
    say "reusing the UFS2 disk (UFS_DEMO_REBUILD=1 recreates it)"
fi
[ -f "$disk" ] || die "missing disk $disk"

head=$(git rev-parse HEAD 2>/dev/null || echo unknown)
if [ "$rebuild" != 1 ] && [ -f "$image" ] && [ -f "$stamp" ] &&
    [ "$(cat "$stamp")" = "$head" ]; then
    say "reusing image $image"
else
    say "building ufs-demo image at $head"
    cargo xtask image --arch "$arch" --profile ufs-demo --output "$image"
    printf '%s\n' "$head" > "$stamp"
fi
[ -f "$image" ] || die "missing image $image"

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$code" ] || die "OVMF code not found: $code (set RAVEN_OVMF_CODE)"
[ -f "$vars" ] || die "OVMF vars not found: $vars (set RAVEN_OVMF_VARS)"
vars_copy=$out/OVMF_VARS.fd
cp "$vars" "$vars_copy"

say "image:      $image"
say "disk:       $disk"
say "commit:     $head"
say "transcript: $log"
cat <<'EOF'
qemu-ufs: guest recipe
  login: root / raven
  lsblk
  gpart show block:0
  mount ufs: /mnt
  echo hello > /mnt/demo.txt
  cat /mnt/demo.txt
  umount /mnt
  exit
qemu-ufs: run this script again to mount the same disk and read /mnt/demo.txt.
EOF

# The guest `exit 0` reaches isa-debug-exit as status 33; map that documented success code to 0
# and propagate every other QEMU status.
status=0
qemu-system-x86_64 \
    -machine q35 \
    -m 512M \
    -smp 4 \
    -display none \
    -monitor none \
    -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars_copy" \
    -drive "format=raw,file=$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -nic none \
    -netdev user,id=n0 \
    -device virtio-net-pci,netdev=n0,disable-legacy=on \
    -drive "if=none,id=ufs,format=raw,file=$disk" \
    -device virtio-blk-pci,drive=ufs,disable-legacy=on \
    -chardev "stdio,id=ser0,logfile=$log,logappend=off" \
    -serial chardev:ser0 || status=$?

case "$status" in
    0 | 33) exit 0 ;;
    *) exit "$status" ;;
esac
