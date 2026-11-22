#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the S85b SMP memory/paging/TLB stress campaign.
#
# usage: sh tests/smp/memory/run.sh [tcg|kvm2|kvm4] [--iterations N] [--timebox SECONDS]
#                                   [--no-build]
#
# The fixture is built once, the image is assembled once, and each iteration runs the qtest case
# with an explicit image. The first failure stops the campaign and is preserved; no retry hides it.

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
    tcg)
        case_file=tests/smp/memory/smp-memory.toml
        case_name=smp-memory
        churn_case=tests/smp/memory/smp-memory-churn.toml
        churn_name=smp-memory-churn
        kvm_state=disabled
        smp=4
        ;;
    kvm2)
        case_file=tests/smp/memory/smp-memory-kvm2.toml
        case_name=smp-memory-kvm2
        churn_case=tests/smp/memory/smp-memory-churn-kvm2.toml
        churn_name=smp-memory-churn-kvm2
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
        churn_case=tests/smp/memory/smp-memory-churn-kvm4.toml
        churn_name=smp-memory-churn-kvm4
        smp=4
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "smp-memory: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        kvm_state="enabled $(ls -l /dev/kvm)"
        ;;
esac

image=build/smp-memory/smp-memory.img
churn_image=build/smp-memory/smp-memory-churn.img
initfs=build/smp-memory/initfs.img
cmdline='console=ttyS0 test-exit=1 smp_user=on smp-user-trace s24-framecheck'
churn_cmdline='console=ttyS0 test-exit=1 s24-framecheck'
out=build/smp-memory/$mode

if [ "$no_build" -eq 0 ]; then
    sh tests/smp/memory/build.sh
    cargo xtask image --arch x86_64 --profile base --cmdline "$cmdline" \
        --initfs "$initfs" --output "$image"
    cargo xtask image --arch x86_64 --profile base --cmdline "$churn_cmdline init=/churn" \
        --initfs "$initfs" --output "$churn_image"
fi
for required in "$image" "$churn_image" "$initfs" "$case_file" "$churn_case"; do
    if [ ! -f "$required" ]; then
        echo "smp-memory: missing $required (run without --no-build first)" >&2
        exit 2
    fi
done

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
    cp "$run_dir/$case_name.log" "$out/run-$iteration.log"
    iteration=$((iteration + 1))
done

count=$(wc -l < "$durations" | tr -d ' ')
if [ "$count" -eq 0 ]; then
    echo "smp-memory: no iterations completed" >&2
    exit 1
fi

churn_dir=$out/churn
mkdir -p "$churn_dir"
if ! RAVEN_SMP_MEMORY_NO_BUILD=1 \
    cargo run -q -p raven-qtest -- run \
        --case "$churn_case" \
        --arch x86_64 \
        --image "$churn_image" \
        --no-build \
        --out "$churn_dir" \
        > "$churn_dir/stdout.txt" 2>&1; then
    {
        echo "smp-memory: FAIL churn mode=$mode iteration=1 commit=$(git rev-parse HEAD)"
        echo "smp-memory: case=$churn_case image=$churn_image"
        echo "smp-memory: log=$churn_dir/$churn_name.log"
        echo "smp-memory: stdout=$churn_dir/stdout.txt"
        tail -n 30 "$churn_dir/stdout.txt"
    } | tee "$out/failure.txt" >&2
    exit 1
fi
churn_log=$churn_dir/$churn_name.log
churn_baseline=$(awk -F= '/^s24: frames free=/ { value=$2 } /^churn: baseline$/ { print value; exit }' "$churn_log")
awk -F= '/^churn: start cycles=10000$/ { started=1; next } started && /^s24: frames free=/ { print $2 }' "$churn_log" > "$churn_dir/frames.txt"
churn_count=$(wc -l < "$churn_dir/frames.txt" | tr -d ' ')
if [ -z "$churn_baseline" ] || [ "$churn_count" -lt 10000 ]; then
    {
        echo "smp-memory: FAIL churn-frames mode=$mode log=$churn_log"
        echo "smp-memory: baseline=${churn_baseline:-missing} frames=$churn_count want-at-least=10000"
    } | tee "$out/failure.txt" >&2
    exit 1
fi
churn_c1000=$(sed -n '1000p' "$churn_dir/frames.txt")
churn_c5000=$(sed -n '5000p' "$churn_dir/frames.txt")
churn_c10000=$(sed -n '10000p' "$churn_dir/frames.txt")
churn_allowed=$((1052672 + 32768))
for pair in "1000:$churn_c1000" "5000:$churn_c5000" "10000:$churn_c10000"; do
    checkpoint=${pair%%:*}
    value=${pair#*:}
    case $value in
        ''|*[!0-9]*)
            {
                echo "smp-memory: FAIL churn-checkpoint mode=$mode cycle=$checkpoint value=${value:-missing}"
            } | tee "$out/failure.txt" >&2
            exit 1
            ;;
    esac
    if [ "$value" -lt $((churn_baseline - churn_allowed)) ]; then
        {
            echo "smp-memory: FAIL churn-reserve mode=$mode cycle=$checkpoint"
            echo "smp-memory: baseline=$churn_baseline value=$value delta=$((churn_baseline - value)) allowed=$churn_allowed"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
done
churn_delta1000=$((churn_baseline - churn_c1000))
churn_delta5000=$((churn_baseline - churn_c5000))
churn_delta10000=$((churn_baseline - churn_c10000))
churn_final=$(tail -n 1 "$churn_dir/frames.txt")
case $churn_final in
    ''|*[!0-9]*)
        {
            echo "smp-memory: FAIL churn-final mode=$mode value=${churn_final:-missing}"
        } | tee "$out/failure.txt" >&2
        exit 1
        ;;
esac
churn_delta_final=$((churn_baseline - churn_final))
if [ "$churn_final" -lt $((churn_baseline - churn_allowed)) ]; then
    {
        echo "smp-memory: FAIL churn-final-reserve mode=$mode"
        echo "smp-memory: baseline=$churn_baseline final=$churn_final delta=$churn_delta_final allowed=$churn_allowed"
    } | tee "$out/failure.txt" >&2
    exit 1
fi
if [ "$churn_delta5000" -gt $((churn_delta1000 + 32768)) ] || \
    [ "$churn_delta10000" -gt $((churn_delta1000 + 32768)) ]; then
    {
        echo "smp-memory: FAIL churn-growth mode=$mode"
        echo "smp-memory: delta1k=$churn_delta1000 delta5k=$churn_delta5000 delta10k=$churn_delta10000 allowed-growth=32768"
    } | tee "$out/failure.txt" >&2
    exit 1
fi
churn_line="smp-memory: churn checkpoints baseline=$churn_baseline c1000=$churn_c1000 c5000=$churn_c5000 c10000=$churn_c10000 final=$churn_final delta1k=$churn_delta1000 delta5k=$churn_delta5000 delta10k=$churn_delta10000 deltaFinal=$churn_delta_final allowed=$churn_allowed"
printf '%s\n' "$churn_line" | tee "$out/churn.txt"

sort -n "$durations" > "$out/durations-ms.sorted"
worst=$(tail -n 1 "$out/durations-ms.sorted")
rank=$(awk -v n="$count" 'BEGIN { print int((n * 90 + 99) / 100) }')
p90=$(sed -n "${rank}p" "$out/durations-ms.sorted")
average=$(awk '{ sum += $1 } END { if (NR) printf "%.0f", sum / NR }' "$durations")
last=$(grep -h '^smp-memory: summary ' "$out"/run-*.log | tail -n 1)

{
    echo "S85b SMP memory/paging/TLB stress"
    echo "mode=$mode smp=$smp kvm=$kvm_state"
    echo "commit=$(git rev-parse HEAD)"
    echo "qemu=$(qemu-system-x86_64 --version | head -n 1)"
    echo "case=$case_file image=$image"
    echo "command=sh tests/smp/memory/run.sh $mode --iterations $iterations --timebox $timebox"
    echo "iterations=$count planned=$iterations timeboxed=$timeboxed timebox_secs=$timebox"
    echo "duration_ms worst=$worst p90=$p90 average=$average"
    echo "last_sample=$last"
    echo "$churn_line"
} | tee "$out/summary.txt"
