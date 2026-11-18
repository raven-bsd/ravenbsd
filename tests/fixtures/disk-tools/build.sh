#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S98 virtio-blk scratch disk: a 64 MiB raw image with one GPT partition.
# No filesystem is written here; the guest newfs formats partition 1.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/disk-tools
mkdir -p "$out"
image=$out/scratch.img

echo "disk-tools: creating $image (64 MiB, GPT partition 1)"
rm -f "$image"
truncate -s 64M "$image"
sgdisk --clear \
    --new=1:2048:131038 \
    --typecode=1:8300 \
    --change-name=1:ufs-scratch \
    "$image"
sgdisk --verify "$image"
echo "disk-tools: scratch disk ready"
