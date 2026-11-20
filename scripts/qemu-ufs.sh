#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K193: build the one-disk bootable UFS demo and run it. The image is a single GPT
# with the FAT ESP first and the 256 MiB UFS2 root second; build/x86_64/ravenbsd-ufs-demo
# carries the qcow2, run.sh and README. `make qemu-ufs` is this script with no flags.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

arch=x86_64
demo=build/$arch/ravenbsd-ufs-demo
raw=$demo/ravenbsd-ufs-demo.raw

# The bootable UFS root carries the same static disk tools as the S99 demo.
if [ "${QEMU_UFS_SKIP_TOOLS:-0}" != 1 ]; then
    sh scripts/ufs-demo.sh --prepare-tools
fi

mkdir -p "$demo"
cargo xtask image --arch "$arch" --profile qemu-ufs --output "$raw"
exec "$demo/run.sh" "$@"
