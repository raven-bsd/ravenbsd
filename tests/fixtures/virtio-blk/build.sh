#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the S205 virtio-blk initfs. The case boots the `base` profile and passes this image as
# its initfs override; the image stages the existing virtio-blk-test profile userland (pcid,
# devd, the Rust virtio-blk driver, blockd and the K15 Rust probe) plus the K15 GPT disk.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "virtio-blk fixture: x86_64 only" >&2
    exit 77
fi

out=build/fixtures/virtio-blk
mkdir -p "$out"

sh tests/fixtures/k15-blk/build.sh
cargo xtask initfs --arch "$arch" --profile virtio-blk-test > /dev/null
cp "build/$arch/initfs.img" "$out/initfs.img"
