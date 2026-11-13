#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the resolv-test initfs with release user programs. The debug initfs is
# large enough that the strict userspace initfs CRC pass can exceed the service
# readiness timeout; the qtest installs this prepared image instead.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b15-resolv
mkdir -p "$out"

RAVEN_USER_RELEASE=1 cargo xtask image \
    --arch x86_64 \
    --profile resolv-test \
    --output "$out/image.img"

cp build/x86_64/initfs.img "$out/initfs.img"
