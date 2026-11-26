#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the S205 virtio-blk initfs. The case boots the `base` profile and passes this image as its
# initfs override; the image stages the Rust userland explicitly (init, pcid, devd, the virtio-blk
# driver and the S205 Rust probe) with the product devd entry plus the `platform-root` grant.

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

cargo xtask build user --arch x86_64 --release \
    -p raven-init -p raven-devd -p raven-pcid -p virtio-blk

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build --manifest-path tests/fixtures/virtio-blk/probe/Cargo.toml \
    --target x86_64-unknown-none --target-dir "$out/probe-target"
cp "$out/probe-target/x86_64-unknown-none/debug/s205-virtio-blk-probe" "$out/probe"
rm -f tests/fixtures/virtio-blk/probe/Cargo.lock

cargo run -q --manifest-path tests/fixtures/virtio-blk/mkdisk/Cargo.toml \
    --target-dir "$out/mkdisk-target" -- "$out/disk.img"
rm -f tests/fixtures/virtio-blk/mkdisk/Cargo.lock

user=build/x86_64/user
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$user/raven-init:0755" \
    "/sbin/pcid=$user/raven-pcid:0755" \
    "/sbin/devd=$user/raven-devd:0755" \
    "/sbin/virtio-blk=$user/virtio-blk:0755" \
    "/sbin/s205-probe=$out/probe:0755" \
    "/etc/services.d/pcid.toml=$root/etc/services.d/pcid.toml:0644" \
    "/etc/services.d/devd.toml=$root/tests/fixtures/virtio-blk/devd.manifest:0644" \
    "/etc/services.d/s205-probe.toml=$root/tests/fixtures/virtio-blk/probe.manifest:0644" \
    "/etc/drivers.toml=$root/tests/fixtures/virtio-blk/drivers.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/virtio-blk/svcd.conf:0644"
