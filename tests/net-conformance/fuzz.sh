#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K548 bounded network fuzz pre-step.  It builds the K253 packet-level listener image, then runs
# the deterministic raw-frame fuzz table against the guest over the private tap.  The harness owns
# the per-case RFC assertions and the summary line; this script only builds, bounds and preserves
# the evidence.  The qtest case then boots the same image and asserts the listener is alive.
#
# usage: sh tests/net-conformance/fuzz.sh [--no-build]
#   --no-build  reuse build/qtest/net-conformance.img (the image must already exist)

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

no_build=0
if [ "${1:-}" = "--no-build" ]; then
    no_build=1
elif [ "$#" -gt 0 ]; then
    echo "usage: $0 [--no-build]" >&2
    exit 2
fi

out=build/net-fuzz
image=build/qtest/net-conformance.img
pid_file=$out/fuzz-serial.qemu.pid
mkdir -p "$out"

# A timeout can kill the Python harness before its QEMU finally block.  Reap only this run's
# QEMU through its pid file so an interrupted fuzz never leaves the image write lock held.
cleanup() {
    if [ -f "$pid_file" ]; then
        pid=$(cat "$pid_file" 2>/dev/null || true)
        if [ -n "$pid" ]; then
            kill -KILL "-$pid" 2>/dev/null || true
            kill -KILL "$pid" 2>/dev/null || true
        fi
        rm -f "$pid_file"
    fi
}
trap cleanup EXIT INT TERM
rm -f "$pid_file"

if [ "$no_build" -eq 0 ]; then
    # A cold network-config-test initfs compiles libc and uutils; the measured cold cap is
    # 600 s.  The fuzz run that follows has its own 55 s cap.
    if ! timeout 600 sh scripts/net-conformance.sh --target raven --build-only \
        > "$out/build.log" 2>&1; then
        echo "net-fuzz: FAIL image build (see $out/build.log)" >&2
        tail -n 30 "$out/build.log" >&2
        exit 1
    fi
fi
if [ ! -f "$image" ]; then
    echo "net-fuzz: FAIL missing $image (run without --no-build)" >&2
    exit 1
fi

started=$(date +%s)
if ! timeout 55 sh scripts/net-conformance.sh --target raven --no-build --image "$image" \
    --scenario fuzz \
    --pcap "$out/fuzz.pcap" \
    --serial "$out/fuzz-serial.log" \
    > "$out/fuzz.log" 2>&1; then
    echo "net-fuzz: FAIL bounded run (see $out/fuzz.log)" >&2
    tail -n 40 "$out/fuzz.log" >&2
    exit 1
fi
elapsed=$(( $(date +%s) - started ))

# The qtest case logs in and exits, so its copy needs test-exit=1.  The fuzz run above keeps the
# profile's normal cmdline.  This is a cached xtask image build after the fixture initfs exists.
if [ "$no_build" -eq 0 ]; then
    if ! timeout 120 cargo xtask image --arch x86_64 --profile network-config-test \
        --initfs build/fixtures/net-conformance/initfs.img \
        --cmdline 'console=ttyS0 test-exit=1' \
        --output build/qtest/net-fuzz.img > "$out/qtest-image.log" 2>&1; then
        echo "net-fuzz: FAIL qtest image build (see $out/qtest-image.log)" >&2
        tail -n 30 "$out/qtest-image.log" >&2
        exit 1
    fi
fi

grep '^net-conformance: fuzz: ' "$out/fuzz.log" > "$out/summary.txt" || true
summary=$(sed -n 's/^net-conformance: fuzz: summary /summary /p' "$out/fuzz.log" | tail -n 1)
if [ -z "$summary" ]; then
    echo "net-fuzz: FAIL no summary line (see $out/fuzz.log)" >&2
    exit 1
fi
echo "net-fuzz: PASS elapsed=${elapsed}s $summary"
echo "net-fuzz: pcap=$out/fuzz.pcap serial=$out/fuzz-serial.log"
