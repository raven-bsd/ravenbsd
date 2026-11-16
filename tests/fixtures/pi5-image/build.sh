#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build a small synthetic FAT boot image for the R8 Pi 5 manifest check and run
# `scripts/pi5-serial.sh --check` against it. This is a host-side fixture, not a bootable Pi 5
# image: the kernel payload is a synthetic non-zero blob and the DTB is our own test tree. The
# real firmware files stay in the read-only donor checkout and are never committed.
#
# The FAT volume starts at the same 1 MiB offset as the K35/R8 GPT layout, so the checker's
# `image@@1M` mtools access is exercised.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi5-image
image=$out/pi5-boot.img

command -v mformat >/dev/null 2>&1 ||
    { echo "pi5-image: mtools (mformat) is required" >&2; exit 1; }
command -v mcopy >/dev/null 2>&1 ||
    { echo "pi5-image: mtools (mcopy) is required" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"

# A raw image with the FAT volume at the 1 MiB offset the manifest checker expects.
dd if=/dev/zero of="$image" bs=1M count=16 2>/dev/null
export MTOOLS_SKIP_CHECK=1
mformat -i "$image@@1M" -v PI5BOOT ::

cat > "$out/config.txt" <<'EOF'
arm_64bit=1
kernel=kernel_2712.img
device_tree=bcm2712-rpi-5-b.dtb
enable_uart=1
uart_2ndstage=1
EOF

printf 'RavenBSD synthetic kernel_2712.img fixture\n' > "$out/kernel_2712.img"
dd if=/dev/zero bs=1024 count=128 >> "$out/kernel_2712.img" 2>/dev/null
cp tests/fixtures/pi5/pi5-valid.dtb "$out/bcm2712-rpi-5-b.dtb"
dd if=/dev/zero of="$out/start4.elf" bs=1024 count=1024 2>/dev/null
dd if=/dev/zero of="$out/fixup4.dat" bs=1024 count=1 2>/dev/null
printf 'synthetic Pi 5 licence fixture\n' > "$out/LICENCE.broadcom"

for file in config.txt kernel_2712.img bcm2712-rpi-5-b.dtb start4.elf fixup4.dat LICENCE.broadcom
do
    mcopy -o -i "$image@@1M" "$out/$file" "::$file"
done

echo "pi5-image: fixture $image"
sh scripts/pi5-serial.sh --check -i "$image"
