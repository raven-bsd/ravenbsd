#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K548 bounded network fuzz pre-step.  It builds the K253 packet-level listener image, then runs
# the selected deterministic raw-frame fuzz table against the guest over the private tap.  The
# harness owns the per-case RFC assertions and the summary line; this script only builds, bounds
# and preserves the evidence.  The qtest case then boots the same image and asserts the listener
# is alive.
#
# usage: sh tests/net-conformance/fuzz.sh [--release] [--no-build] [--scenario fuzz|l4|frag-timeout]
#   --release   run the shipped `release` profile with the test listener overlay
#   --no-build  reuse the stamped image
#   --scenario  fuzz (TCP), l4 (UDP/ICMP/IPv4 options/fragments), or frag-timeout

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

no_build=0
release=0
scenario=fuzz
while [ "$#" -gt 0 ]; do
    case "$1" in
        --release)
            release=1
            shift
            ;;
        --no-build)
            no_build=1
            shift
            ;;
        --scenario)
            [ "$#" -ge 2 ] || { echo "--scenario needs a value" >&2; exit 2; }
            scenario=$2
            shift 2
            ;;
        *)
            echo "usage: $0 [--release] [--no-build] [--scenario fuzz|l4|frag-timeout]" >&2
            exit 2
            ;;
    esac
done

# Functional rows, not latency rows: the cap covers the guest boot plus the
# campaign.  Measured on c2892c717 at load1=20.47: the TCP campaign was 29.50 s
# and the whole pre-step under 55 s.  Gate 2 at qemu=3/load1=15.81 cut the
# campaign at 24.5 s of group time with the old 55 s cap, so the row stopped
# before its summary.  A 120 s cap is ~2.4x the measured quiet pre-step and
# stays well inside the pre-step's own 900 s cap.  The per-case wall guard in
# harness.py is measured separately; this cap is the campaign bound.
case "$scenario" in
    fuzz)
        harness_scenario=fuzz
        label=net-fuzz
        run_cap=120
        ;;
    l4)
        harness_scenario=fuzz-l4
        label=net-fuzz-l4
        run_cap=120
        ;;
    frag-timeout)
        harness_scenario=fuzz-frag-timeout
        label=net-fuzz-frag-timeout
        run_cap=120
        ;;
    churn)
        harness_scenario=churn
        label=net-churn
        run_cap=120
        ;;
    *)
        echo "unknown scenario: $scenario" >&2
        exit 2
        ;;
esac

out=build/net-fuzz
if [ "$release" -eq 1 ]; then
    profile=release
    fixture=build/fixtures/net-conformance-release
    image=build/qtest/net-conformance-release.img
    qtest_image=build/qtest/net-fuzz-release.img
    stamp=$out/image-release.stamp
    tag=release
else
    profile=network-config-test
    fixture=build/fixtures/net-conformance
    image=build/qtest/net-conformance.img
    qtest_image=build/qtest/net-fuzz.img
    stamp=$out/image.stamp
    tag=
fi
if [ "$scenario" = fuzz ]; then
    log=$out/fuzz${tag:+-$tag}.log
    pcap=$out/fuzz${tag:+-$tag}.pcap
    serial=$out/fuzz${tag:+-$tag}-serial.log
    summary_file=$out/summary${tag:+-$tag}.txt
else
    log=$out/fuzz-${scenario}${tag:+-$tag}.log
    pcap=$out/fuzz-${scenario}${tag:+-$tag}.pcap
    serial=$out/fuzz-${scenario}${tag:+-$tag}-serial.log
    summary_file=$out/summary-${scenario}${tag:+-$tag}.txt
fi
if [ "$release" -eq 1 ]; then
    label=$label-release
fi
export NETCONF_PROFILE="$profile"
export NETCONF_OUT="$fixture"
export NETCONF_IMAGE="$image"
pid_file=${serial%.log}.qemu.pid
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

# Reuse a build only when the product tree state is byte-for-byte the one that produced the stamp.
# The three gate rows then share one cold build; a source edit invalidates it.
if [ "$no_build" -eq 0 ]; then
    head=$(git rev-parse HEAD)
    dirty=$(git status --porcelain | sha256sum | cut -d' ' -f1)
    stamp_value="$head:$dirty"
    if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$stamp_value" ] &&
        [ -f "$image" ] && [ -f "$qtest_image" ]; then
        no_build=1
        echo "$label: reusing image stamp $head"
    fi
fi

if [ "$no_build" -eq 0 ]; then
    # A cold profile initfs compiles libc and uutils; the measured cold cap is 600 s.  The
    # fuzz run that follows has its own scenario cap.
    if ! timeout 600 sh scripts/net-conformance.sh --target raven --build-only \
        > "$out/build${tag:+-$tag}.log" 2>&1; then
        echo "$label: FAIL image build (see $out/build${tag:+-$tag}.log)" >&2
        tail -n 30 "$out/build${tag:+-$tag}.log" >&2
        exit 1
    fi
fi
for required in "$image"; do
    if [ ! -f "$required" ]; then
        echo "$label: FAIL missing $required (run without --no-build)" >&2
        exit 1
    fi
done

started=$(date +%s)
if ! timeout "$run_cap" sh scripts/net-conformance.sh --target raven --no-build \
    --image "$image" \
    --scenario "$harness_scenario" \
    --pcap "$pcap" \
    --serial "$serial" \
    > "$log" 2>&1; then
    echo "$label: FAIL bounded run (see $log)" >&2
    tail -n 40 "$log" >&2
    exit 1
fi
elapsed=$(( $(date +%s) - started ))

# The qtest case logs in and exits, so its copy needs test-exit=1.  The fuzz run above keeps the
# profile's normal cmdline.  This is a cached xtask image build after the fixture initfs exists.
if [ "$no_build" -eq 0 ]; then
    if ! timeout 120 cargo xtask image --arch x86_64 --profile "$profile" \
        --initfs "$fixture/initfs.img" \
        --cmdline 'console=ttyS0 test-exit=1' \
        --output "$qtest_image" > "$out/qtest-image${tag:+-$tag}.log" 2>&1; then
        echo "$label: FAIL qtest image build (see $out/qtest-image${tag:+-$tag}.log)" >&2
        tail -n 30 "$out/qtest-image${tag:+-$tag}.log" >&2
        exit 1
    fi
    printf '%s\n' "$stamp_value" > "$stamp"
fi

grep "^net-conformance: ${harness_scenario}: " "$log" > "$summary_file" || true
summary=$(sed -n "s/^net-conformance: ${harness_scenario}: summary /summary /p" "$log" | tail -n 1)
if [ -z "$summary" ]; then
    echo "$label: FAIL no summary line (see $log)" >&2
    exit 1
fi
echo "$label: PASS elapsed=${elapsed}s $summary"
echo "$label: pcap=$pcap serial=$serial"
