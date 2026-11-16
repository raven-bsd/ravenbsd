#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K27c update-health fixtures. The fixture disk carries an ESP with boot.toml and the
# A/B slot payloads plus a UFS root partition; the health record lives in the UFS root and the
# marker is at LBA 34 on the same disk. The trial marker is healthy, stale with a valid trial
# kernel, or stale with an invalid kernel depending on the case.

set -eu

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        linker=userland/linker/x86_64.ld
        probe_flags="-C code-model=small -C relocation-model=static -C lto=no"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        linker=userland/linker/aarch64.ld
        probe_flags="-C relocation-model=static -C force-frame-pointers=yes"
        ;;
    *)
        echo "update-health: unsupported arch $arch" >&2
        exit 1
        ;;
esac

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

target_dir=${CARGO_TARGET_DIR:-$root/target}
mkdir -p build/qtest
printf 'fixture initfs' > build/qtest/update-health-initfs.img

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes" \
    cargo build -p raven-testkernel --target "$target" --release
testkernel="$target_dir/$target/release/raven-testkernel"

cargo xtask build kernel --arch "$arch"
cargo xtask initfs --arch "$arch" --profile update-health

RUSTFLAGS="$probe_flags -C link-arg=-T$root/$linker" \
    cargo build -p raven-update-health-probe --target "$target" --release
probe="$target_dir/$target/release/raven-update-health-probe"

staging=build/qtest/update-health-root
rm -rf "$staging"
mkdir -p "$staging/sbin" "$staging/etc/services.d"
cp "$probe" "$staging/sbin/health-probe"
cp tests/fixtures/update/root/etc/services.d/health-probe.toml \
    "$staging/etc/services.d/health-probe.toml"

root_size=8388608
cargo xtask update --fixture build/qtest/update-health-good.img \
    --arch "$arch" --health good \
    --kernel "build/$arch/kernel.elf" --initfs "build/$arch/initfs.img" \
    --base-kernel "$testkernel" --base-initfs build/qtest/update-health-initfs.img \
    --root-staging "$staging" --root-size "$root_size"

cargo xtask update --fixture build/qtest/update-health-trial.img \
    --arch "$arch" --health trial \
    --kernel "build/$arch/kernel.elf" --initfs "build/$arch/initfs.img" \
    --base-kernel "$testkernel" --base-initfs build/qtest/update-health-initfs.img \
    --root-staging "$staging" --root-size "$root_size"

cargo xtask update --fixture build/qtest/update-health-stale.img \
    --arch "$arch" --health bad \
    --kernel "$testkernel" --initfs build/qtest/update-health-initfs.img \
    --root-staging "$staging" --root-size "$root_size"

cargo xtask update --fixture build/qtest/update-rollback-slots.img \
    --arch "$arch" --health bad \
    --kernel "$testkernel" --initfs build/qtest/update-health-initfs.img
