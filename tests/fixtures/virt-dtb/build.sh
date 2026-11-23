#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the arch-selected net profile inputs for the release image:
#   build/fixtures/virt-dtb/virt.dtb          QEMU `virt` tree (virtio_mmio@a000000)
#   build/fixtures/virt-dtb/<arch>/devd.manifest
#   build/fixtures/virt-dtb/<arch>/drivers.conf
# The profile marks these generated, so Profile::load does not validate the literal {arch} source;
# the initfs builder resolves the expanded build path. Run before `cargo xtask image`.
#
# Usage: sh tests/fixtures/virt-dtb/build.sh [x86_64|aarch64]

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64 | aarch64) ;;
    *)
        echo "virt-dtb: unsupported arch $arch (x86_64 or aarch64)" >&2
        exit 2
        ;;
esac

out=build/fixtures/virt-dtb/$arch
mkdir -p "$out"
python3 tests/fixtures/virt-dtb/generate.py
cp "profiles/release.d/devd-net-$arch.manifest" "$out/devd.manifest"
cp "profiles/release.d/release-drivers-$arch.conf" "$out/drivers.conf"
cp "profiles/release.d/pcid-net-$arch.manifest" "$out/pcid.toml"
printf 'virt-dtb: staged %s (%s)\n' "$out" "$arch"
