#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the smallest donor program from the `openbsd-libc` exit-status suite against the
# ravenbsd sysroot and pack it as `/init` for the `g1-openbsd` qtest. The full suite is run by
# `raven-atf-run suite --run-backend qemu`; this case is the boot-level smoke test.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/g1
conformance=$out/conformance
mkdir -p "$out"

cargo run -q -p raven-atf-run -- suite \
    --suite openbsd-libc \
    --mode target \
    --sysroot build/x86_64/sysroot \
    --run-backend none \
    --filter 'strnlen/*' \
    --out "$conformance"

program=$conformance/target/openbsd-libc/strnlen/strnlentest/program
if [ ! -f "$program" ]; then
    printf 'g1 fixture: target program missing: %s\n' "$program" >&2
    exit 1
fi

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$program"
