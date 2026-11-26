#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K527 run-queue saturation campaign.
#
# usage: sh tests/smp/sched/run.sh [smp1|kvm2|kvm4] [--iterations N] [--timebox SECONDS]
#                                  [--no-build]
#
# Each iteration boots the kernel with `selftest=sched-saturation`: 80 kernel threads at one
# priority sleep, wake and yield through 256 schedules each. The monitor requires every worker to
# complete and at least 10,000 switches. The first failure stops the campaign and is preserved.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-smp1}
case "$mode" in
    smp1|kvm2|kvm4) ;;
    *)
        printf 'usage: %s [smp1|kvm2|kvm4] [--iterations N] [--timebox SECONDS] [--no-build]\n' "$0" >&2
        exit 2
        ;;
esac
shift || true

iterations=1
timebox=900
no_build=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --iterations)
            [ "$#" -ge 2 ] || { echo "--iterations needs a value" >&2; exit 2; }
            iterations=$2
            shift 2
            ;;
        --timebox)
            [ "$#" -ge 2 ] || { echo "--timebox needs a value" >&2; exit 2; }
            timebox=$2
            shift 2
            ;;
        --no-build)
            no_build=1
            shift
            ;;
        *)
            echo "unknown option: $1" >&2
            exit 2
            ;;
    esac
done

case "$mode" in
    smp1)
        case_file=tests/smp/sched/sched-saturation-smp1.toml
        case_name=sched-saturation-smp1
        ;;
    kvm2)
        case_file=tests/smp/sched/sched-saturation-kvm2.toml
        case_name=sched-saturation-kvm2
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "sched-saturation: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        ;;
    kvm4)
        case_file=tests/smp/sched/sched-saturation-kvm4.toml
        case_name=sched-saturation-kvm4
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "sched-saturation: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        ;;
esac

out=build/smp-sched/$mode
rm -rf "$out"
mkdir -p "$out"
durations=$out/durations-ms.txt
: > "$durations"

start=$(date +%s)
iteration=1
timeboxed=no
while [ "$iteration" -le "$iterations" ]; do
    now=$(date +%s)
    elapsed=$((now - start))
    if [ "$elapsed" -ge "$timebox" ]; then
        timeboxed=yes
        break
    fi
    run_dir=$out/run-$iteration
    mkdir -p "$run_dir"
    if [ "$no_build" -eq 1 ]; then
        no_build_arg=--no-build
    else
        no_build_arg=
    fi
    if ! cargo run -q -p raven-qtest -- run \
        --case "$case_file" \
        --arch x86_64 \
        $no_build_arg \
        --out "$run_dir" \
        > "$run_dir/stdout.txt" 2>&1; then
        {
            echo "sched-saturation: FAIL mode=$mode iteration=$iteration commit=$(git rev-parse HEAD)"
            echo "sched-saturation: case=$case_file"
            echo "sched-saturation: stdout=$run_dir/stdout.txt"
            tail -n 30 "$run_dir/stdout.txt"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    duration=$(sed -n 's/.*"duration_ms": \([0-9][0-9]*\).*/\1/p' "$run_dir/$case_name.json" | head -n 1)
    if [ -z "$duration" ]; then
        echo "sched-saturation: missing duration in $run_dir/$case_name.json" >&2
        exit 2
    fi
    printf '%s\n' "$duration" >> "$durations"
    echo "sched-saturation: pass mode=$mode iteration=$iteration duration_ms=$duration"
    iteration=$((iteration + 1))
done

count=$(wc -l < "$durations" | tr -d ' ')
worst=$(sort -n "$durations" | tail -n 1)
echo "sched-saturation: PASS mode=$mode iterations=$count worst_ms=$worst timeboxed=$timeboxed"
