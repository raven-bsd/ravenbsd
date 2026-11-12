#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-side oracle between the two boots: extract the UFS partition from the disk the first boot
# wrote and require `ufs-fsck -n` to report clean. A dirty image fails the case with the output.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k72-hwfs
if [ ! -f "$out/disk.img" ]; then
    echo "hw-fs2: $out/disk.img is missing; run the hw-fs case first" >&2
    exit 1
fi

cargo run -q -p raven-k72-mkdisk -- extract "$out/disk.img" "$out/root.fsck.ufs"
if ! cargo run -q -p ufs-fsck -- -n "$out/root.fsck.ufs" > "$out/fsck.txt" 2>&1; then
    cat "$out/fsck.txt" >&2
    exit 1
fi
