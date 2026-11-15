# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# Build the K27a health fixtures: the normal image supplies the loader; the second GPT disk
# supplies slot A (good test kernel), slot B and a marker sector. The good fixture carries a
# matching marker; the bad fixture's marker is stale and slot B is also an invalid kernel.

set -eu

mkdir -p build/qtest
printf 'fixture initfs' > build/qtest/update-health-initfs.img

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes" \
    cargo build -p raven-testkernel --target x86_64-unknown-none --release

cargo xtask update --fixture build/qtest/update-health-good.img \
    --arch x86_64 --health good \
    --kernel target/x86_64-unknown-none/release/raven-testkernel \
    --initfs build/qtest/update-health-initfs.img

cargo xtask update --fixture build/qtest/update-health-bad-slots.img \
    --arch x86_64 --health bad \
    --kernel target/x86_64-unknown-none/release/raven-testkernel \
    --initfs build/qtest/update-health-initfs.img

cargo xtask update --fixture build/qtest/update-rollback-slots.img \
    --arch x86_64 --health bad \
    --kernel target/x86_64-unknown-none/release/raven-testkernel \
    --initfs build/qtest/update-health-initfs.img

cargo xtask update --blank build/qtest/update-health-scratch.img
