#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the update-health good-marker fixture for the architecture under test.
# raven-qtest exports RAVEN_QT_ARCH from --arch; the test kernel target follows it.

set -eu

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) target=x86_64-unknown-none ;;
    aarch64) target=aarch64-unknown-none-softfloat ;;
    *)
        echo "update-health: unsupported arch $arch" >&2
        exit 1
        ;;
esac

mkdir -p build/qtest
printf 'fixture initfs' > build/qtest/update-health-initfs.img

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes" \
    cargo build -p raven-testkernel --target "$target" --release

cargo xtask update --fixture build/qtest/update-health-good.img \
    --arch "$arch" --health good \
    --kernel "target/$target/release/raven-testkernel" \
    --initfs build/qtest/update-health-initfs.img
