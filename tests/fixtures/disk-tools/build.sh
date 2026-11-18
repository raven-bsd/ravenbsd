#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S110 virtio-blk scratch disk: a 16 MiB blank payload wrapped in a one-partition GPT
# by the repository's own `raven-k54-mkimage`. No filesystem is written here; the guest `newfs`
# formats the device, and the guest `lsblk`/`gpart` read the real GPT before that.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/disk-tools
mkdir -p "$out"
payload=$out/blank.img
image=$out/scratch.img

echo "disk-tools: creating $image (16 MiB blank payload + GPT)"
rm -f "$payload" "$image"
truncate -s 16M "$payload"
cargo run -q -p raven-k54-mkimage -- "$payload" "$image"
rm -f "$payload"
echo "disk-tools: scratch disk ready"
