#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the S85b SMP memory/paging/TLB stress campaign.
#
# usage: sh tests/smp/memory/run.sh [tcg|kvm2|kvm4] [--iterations N] [--timebox SECONDS]
#                                   [--max-load N] [--no-build]
#
# The fixture is built once, the image is assembled once, and each iteration runs the qtest case
# with an explicit image. The first failure stops the campaign and is preserved; no retry hides it.
# `--max-load N` refuses to start when the host 1-minute load exceeds N (`0` disables the guard);
# a starved host stretches a ~100 s case past its 300 s qtest budget and must not be recorded as a
# product failure. Every iteration records load1 before/after in `<out>/load.tsv`.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-tcg}
case "$mode" in
    tcg|kvm2|kvm4) ;;
    *) printf 'usage: %s [tcg|kvm2|kvm4] [--iterations N] [--timebox SECONDS] [--no-build]\n' "$0" >&2; exit 2 ;;
esac
shift || true

iterations=50
timebox=2700
no_build=0
max_load=0
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
        --max-load)
            [ "$#" -ge 2 ] || { echo "--max-load needs a value" >&2; exit 2; }
            max_load=$2
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

case $max_load in
    ''|*[!0-9]*) echo "--max-load needs a non-negative integer" >&2; exit 2 ;;
esac

if [ "$max_load" -gt 0 ]; then
    load1=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
    if awk -v load1v="$load1" -v maxv="$max_load" 'BEGIN { exit !(load1v > maxv) }'; then
        echo "smp-memory: REFUSED load1=$load1 > max_load=$max_load (busy host; a starved TCG run is not a product result)" >&2
        exit 3
    fi
fi

case "$mode" in
    tcg)
        case_file=tests/smp/memory/smp-memory.toml
        case_name=smp-memory
        kvm_state=disabled
        smp=4
        ;;
    kvm2)
        case_file=tests/smp/memory/smp-memory-kvm2.toml
        case_name=smp-memory-kvm2
        smp=2
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "smp-memory: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        kvm_state="enabled $(ls -l /dev/kvm)"
        ;;
    kvm4)
        case_file=tests/smp/memory/smp-memory-kvm4.toml
        case_name=smp-memory-kvm4
        smp=4
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "smp-memory: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        kvm_state="enabled $(ls -l /dev/kvm)"
        ;;
esac

image=build/smp-memory/smp-memory.img
initfs=build/smp-memory/initfs.img
cmdline='console=ttyS0 test-exit=1 smp_user=on smp-user-trace s24-framecheck'
out=build/smp-memory/$mode

if [ "$no_build" -eq 0 ]; then
    sh tests/smp/memory/build.sh
    cargo xtask image --arch x86_64 --profile base --cmdline "$cmdline" \
        --initfs "$initfs" --output "$image"
fi
for required in "$image" "$initfs" "$case_file"; do
    if [ ! -f "$required" ]; then
        echo "smp-memory: missing $required (run without --no-build first)" >&2
        exit 2
    fi
done

rm -rf "$out"
mkdir -p "$out"
durations=$out/durations-ms.txt
loads=$out/load.tsv
: > "$durations"
printf 'iteration\tload1_before\tload1_after\tduration_ms\n' > "$loads"
load_start=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)

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
    if [ "$max_load" -gt 0 ]; then
        load_now=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
        if awk -v load1v="$load_now" -v maxv="$max_load" 'BEGIN { exit !(load1v > maxv) }'; then
            echo "smp-memory: STOP load1=$load_now > max_load=$max_load after $((iteration - 1)) iterations (host starvation, not a product result)" >&2
            exit 3
        fi
    fi
    run_dir=$out/run-$iteration
    mkdir -p "$run_dir"
    load_before=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
    if ! RAVEN_SMP_MEMORY_NO_BUILD=1 \
        cargo run -q -p raven-qtest -- run \
            --case "$case_file" \
            --arch x86_64 \
            --image "$image" \
            --no-build \
            --out "$run_dir" \
            > "$run_dir/stdout.txt" 2>&1; then
        {
            echo "smp-memory: FAIL mode=$mode iteration=$iteration commit=$(git rev-parse HEAD)"
            echo "smp-memory: case=$case_file image=$image"
            echo "smp-memory: host_load1=$load_before"
            echo "smp-memory: log=$run_dir/$case_name.log"
            echo "smp-memory: stdout=$run_dir/stdout.txt"
            tail -n 30 "$run_dir/stdout.txt"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    duration=$(sed -n 's/.*"duration_ms": \([0-9][0-9]*\).*/\1/p' "$run_dir/$case_name.json" | head -n 1)
    if [ -z "$duration" ]; then
        echo "smp-memory: missing duration in $run_dir/$case_name.json" >&2
        exit 2
    fi
    # K475: the s24 evidence line is kernel serial output, so the numeric baseline check lives
    # here in the runner. `baseline` is the last free-frame line before the first OOM round;
    # `post1`/`post2` are the last lines before each round's `done` drain marker. The documented
    # one-time reserve (1,052,672 B from K453) and the test-owned 4 KiB shared barrier page are
    # allowed; a per-round difference is limited to the documented 32 KiB cache transient.
    log=$run_dir/$case_name.log
    baseline=$(awk -F= '/^s24: frames free=/ { value=$2 } /^oom: round=1 children=2/ { print value; exit }' "$log")
    post1=$(awk -F= '/^s24: frames free=/ { value=$2 } /^oom: round=1 done/ { print value; exit }' "$log")
    post2=$(awk -F= '/^s24: frames free=/ { value=$2 } /^oom: round=2 done/ { print value; exit }' "$log")
    for value in "$baseline" "$post1" "$post2"; do
        case $value in
            ''|*[!0-9]*)
                {
                    echo "smp-memory: FAIL oom-baseline mode=$mode iteration=$iteration log=$log"
                    echo "smp-memory: baseline=$baseline post1=$post1 post2=$post2 (missing s24 or oom marker)"
                } | tee "$out/failure.txt" >&2
                exit 1
                ;;
        esac
    done
    allowed=$((4096 + 1052672 + 32768))
    if [ "$post1" -lt $((baseline - allowed)) ]; then
        {
            echo "smp-memory: FAIL oom-baseline mode=$mode iteration=$iteration log=$log"
            echo "smp-memory: baseline=$baseline post1=$post1 delta=$((baseline - post1)) allowed=$allowed"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    if [ "$post2" -lt $((post1 - 32768)) ]; then
        {
            echo "smp-memory: FAIL oom-per-attempt mode=$mode iteration=$iteration log=$log"
            echo "smp-memory: post1=$post1 post2=$post2 delta=$((post1 - post2)) allowed=32768"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    printf '%s\n' "$duration" >> "$durations"
    load_after=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
    printf '%s\t%s\t%s\t%s\n' "$iteration" "$load_before" "$load_after" "$duration" >> "$loads"
    cp "$run_dir/$case_name.log" "$out/run-$iteration.log"
    iteration=$((iteration + 1))
done

count=$(wc -l < "$durations" | tr -d ' ')
if [ "$count" -eq 0 ]; then
    echo "smp-memory: no iterations completed" >&2
    exit 1
fi
sort -n "$durations" > "$out/durations-ms.sorted"
worst=$(tail -n 1 "$out/durations-ms.sorted")
rank=$(awk -v n="$count" 'BEGIN { print int((n * 90 + 99) / 100) }')
p90=$(sed -n "${rank}p" "$out/durations-ms.sorted")
average=$(awk '{ sum += $1 } END { if (NR) printf "%.0f", sum / NR }' "$durations")
last=$(grep -h '^smp-memory: summary ' "$out"/run-*.log | tail -n 1)
load_end=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
load_max=$(awk -F'\t' 'NR > 1 && $2 + 0 > max { max = $2 + 0 } END { printf "%.2f", max }' "$loads")

{
    echo "S85b SMP memory/paging/TLB stress"
    echo "mode=$mode smp=$smp kvm=$kvm_state"
    echo "commit=$(git rev-parse HEAD)"
    echo "qemu=$(qemu-system-x86_64 --version | head -n 1)"
    echo "case=$case_file image=$image"
    echo "command=sh tests/smp/memory/run.sh $mode --iterations $iterations --timebox $timebox --max-load $max_load"
    echo "iterations=$count planned=$iterations timeboxed=$timeboxed timebox_secs=$timebox"
    echo "duration_ms worst=$worst p90=$p90 average=$average"
    echo "host_load1 start=$load_start end=$load_end max=$load_max max_load=$max_load"
    echo "last_sample=$last"
} | tee "$out/summary.txt"
