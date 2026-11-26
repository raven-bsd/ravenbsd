#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the S205 virtio-net initfs. The case boots the `base` profile and passes this image as
# its initfs override; the image stages the existing net-b-test profile userland (pcid, devd, the
# Rust virtio-net frame service and the Rust B39 ring probe). The product devd entry grants
# `platform-root`, which the init policy allows only for `devd`; the test profile's copy predates
# that grant, so the fixture installs the same entry with the grant into the staged initfs.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "virtio-net fixture: x86_64 only" >&2
    exit 77
fi

out=build/fixtures/virtio-net
mkdir -p "$out"

sh tests/fixtures/b12b-net/build.sh
cargo xtask initfs --arch "$arch" --profile net-b-test > /dev/null
cargo build -q -p raven-b70-overlay

target_dir=${CARGO_TARGET_DIR:-$root/target}
"$target_dir/debug/raven-b70-overlay" \
    "build/$arch/initfs.img" \
    "$out/initfs.img" \
    "/etc/services.d/devd.toml=$root/tests/fixtures/virtio-net/devd.manifest"
