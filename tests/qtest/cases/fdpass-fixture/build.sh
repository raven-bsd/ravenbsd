#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K438 fdpass guest image. The C probe is the ordinary libc-test program; the base
# image is the static libc-test profile (it already carries initfsd/consd and the pty server).
# The fixture-only overlay adds the static virtio-net stack, the one-shot probe service and the
# exit hook, so the connected-TCP phase has a real netd stream. Nothing in profiles/ or the
# product binaries changes.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "fdpass fixture: x86_64 only" >&2
    exit 77
fi

case_dir=$root/tests/qtest/cases/fdpass-fixture
out=$root/build/fixtures/fdpass
mkdir -p "$out"

sh "$root/tests/libc/target/build.sh" fdpass

cargo xtask initfs --arch "$arch" --profile libc-test >/dev/null
cp "build/$arch/initfs.img" "$out/base.img"

cargo xtask build user --arch "$arch" --release \
    -p raven-pcid -p raven-devd -p virtio-net -p netd -p raven-ptyd >/dev/null
cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
user=$root/build/$arch/user

"$overlay" "$out/base.img" "$out/fdpass.img" \
    "/bin/libc-test=$root/build/libc-test/libc-test:0755" \
    "/sbin/ptyd=$user/raven-ptyd:0755" \
    "/sbin/pcid=$user/raven-pcid:0755" \
    "/sbin/devd=$user/raven-devd:0755" \
    "/sbin/virtio-net=$user/virtio-net:0755" \
    "/sbin/netd=$user/netd:0755" \
    "/etc/services.d/libc-test.toml=$case_dir/fdpass.toml" \
    "/etc/services.d/pcid.toml=$root/etc/services.d/pcid.toml" \
    "/etc/services.d/devd.toml=$root/profiles/server.d/devd.manifest" \
    "/etc/services.d/netd.toml=$root/rootfs/server/etc/services.d/netd.toml" \
    "/etc/drivers.toml=$root/userland/init/release-drivers.conf" \
    "/etc/raven/conf.d/network.toml=$root/profiles/network-config-test/network.toml" \
    "/etc/svcd.conf=$case_dir/svcd.conf"

echo "fdpass fixture: staged $out/fdpass.img"
