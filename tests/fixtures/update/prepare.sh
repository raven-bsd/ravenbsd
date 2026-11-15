# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

# Build the QEMU rollback fixture: the normal image supplies the loader, the second GPT disk
# supplies the A/B slot store. Slot B holds a deliberately invalid kernel so the loader exhausts
# the trial budget and rolls back to slot A.

set -eu

mkdir -p build/qtest
printf 'fixture initfs' > build/qtest/update-rollback-initfs.img

RUSTFLAGS="-C relocation-model=static -C force-frame-pointers=yes" \
    cargo build -p raven-testkernel --target x86_64-unknown-none --release

cargo xtask update --fixture build/qtest/update-rollback-slots.img \
    --arch x86_64 \
    --kernel target/x86_64-unknown-none/release/raven-testkernel \
    --initfs build/qtest/update-rollback-initfs.img
