#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the picolibc `test-uchar` exit-status program against the ravenbsd sysroot and pack it as
# `/init` for the `g1-picolibc` qtest. This is the C5 side of the G1 qemu backend: the same boot
# path and `test-exit=1` protocol as `g1-openbsd`, with a picolibc program.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/g1
conformance=$out/picolibc-conformance
mkdir -p "$out"

cargo run -q -p raven-atf-run -- suite \
    --suite picolibc \
    --mode target \
    --sysroot build/x86_64/sysroot \
    --run-backend none \
    --filter 'test-uchar' \
    --out "$conformance"

program=$conformance/target/picolibc/test-uchar/program
if [ ! -f "$program" ]; then
    printf 'g1 picolibc fixture: target program missing: %s\n' "$program" >&2
    exit 1
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/picolibc-initfs.img" \
    /init="$program"
