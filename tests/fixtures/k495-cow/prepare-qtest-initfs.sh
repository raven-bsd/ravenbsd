#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S164 test-only: build the k495-cow fixture and a libc-test-profile initfs whose one-shot
# /bin/libc-test service is the fixture. The qtest installs the initfs over the libc-test profile;
# the fixture's default `all` mode runs the fork COW and shared-store assertions and exits.
#
# Usage: sh tests/fixtures/k495-cow/prepare-qtest-initfs.sh
# Output: build/qtest/cow-visibility-initfs.img

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/qtest
mkdir -p "$out"

sh tests/libc/target/build.sh termios > "$out/cow-libc-build.log" 2>&1
sh tests/fixtures/k495-cow/build.sh > "$out/cow-fixture-build.log" 2>&1

if [ ! -x target/debug/raven-b70-overlay ]; then
    cargo build -q -p raven-b70-overlay > "$out/cow-overlay-build.log" 2>&1
fi

cargo xtask initfs --arch x86_64 --profile libc-test > "$out/cow-initfs-build.log" 2>&1

target/debug/raven-b70-overlay build/x86_64/initfs.img "$out/cow-visibility-initfs.img" \
    "/bin/libc-test=$root/build/fixtures/k495-cow/k495-cow:0755" \
    > "$out/cow-overlay.log" 2>&1

echo "cow-visibility: initfs $out/cow-visibility-initfs.img"
