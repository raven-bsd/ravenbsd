#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K376: reproducible load-matched TLB stress gate for S159.
#
# Drives the existing `posix-fs-smp` TCG -smp 4 case for N iterations under a deterministic
# bounded CPU load, recording per-iteration start/end, load 1/5/15, exit code, duration, serial
# log and every tlb/timeout/stale/FAIL/pass line. It also runs the S113 TLB reproducer 10/10
# separately. No retry hides a failure; the run completes the requested rows and exits 1 if any
# row failed.
#
# usage: scripts/tlb-stress.sh [options]
#   --load N        busy workers (default 100; 0 is the low-load control)
#   --iterations N  qtest iterations (default 20)
#   --out DIR       output directory (default build/tlb-stress/<UTC>)
#   --no-prebuild   reuse the existing qtest image (the prebuild is otherwise run once)
#   --no-s113       skip the separate S113 TLB 10/10 run (not recommended)
#   --case NAME     qtest case (default posix-fs-smp; TCG -smp 4)
#   --max-host-load L  max host load for the --load 0 control (default 20)
#   -h, --help      this help
#
# The first prebuild and every qtest run are foreground; the load generator is a child process
# group owned by this script and is always stopped on exit. Per-iteration qtest cap: 420 s
# (case timeout_secs 300 + build margin). S113 cap: 300 s.

set -u
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

LOAD=100
ITERATIONS=20
OUT=build/tlb-stress/$(date -u +%Y%m%dT%H%M%SZ)
PREBUILD=1
RUN_S113=1
S113_SET=0
CASE=posix-fs-smp
QTEST_CAP=420
S113_CAP=300
MAX_HOST_LOAD=20

usage() {
    sed -n '6,26p' "$0" | sed 's/^# \{0,1\}//'
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --load)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --load needs a value" >&2; exit 2; }
            LOAD=$2
            shift 2
            ;;
        --iterations)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --iterations needs a value" >&2; exit 2; }
            ITERATIONS=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --out needs a value" >&2; exit 2; }
            OUT=$2
            shift 2
            ;;
        --case)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --case needs a value" >&2; exit 2; }
            CASE=$2
            shift 2
            ;;
        --max-host-load)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --max-host-load needs a value" >&2; exit 2; }
            MAX_HOST_LOAD=$2
            shift 2
            ;;
        --no-prebuild) PREBUILD=0; shift ;;
        --no-s113) RUN_S113=0; S113_SET=1; shift ;;
        --s113) RUN_S113=1; S113_SET=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *)
            echo "tlb-stress: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$LOAD" in ''|*[!0-9]*) echo "tlb-stress: --load must be a non-negative integer" >&2; exit 2 ;; esac
case "$ITERATIONS" in ''|*[!0-9]*) echo "tlb-stress: --iterations must be a non-negative integer" >&2; exit 2 ;; esac
# The low-load control is the quick acceptance path; the S113 10/10 run belongs to the
# load-matched run unless explicitly requested.
if [ "$LOAD" -eq 0 ] && [ "$S113_SET" -eq 0 ]; then
    RUN_S113=0
fi
if [ "$LOAD" -eq 0 ]; then
    load1=$(cut -d' ' -f1 /proc/loadavg)
    load1=${load1%.*}
    if [ "$load1" -gt "$MAX_HOST_LOAD" ]; then
        echo "tlb-stress: BLOCKED low-load control: host load1=$load1 > $MAX_HOST_LOAD (no samples)" >&2
        exit 2
    fi
fi

mkdir -p "$OUT" || exit 2
PIDFILE=$OUT/load.pids
trap 'sh tests/smp/tlb/load-generator.sh stop "$PIDFILE" >/dev/null 2>&1 || true' EXIT INT TERM

record_host() {
    path=$1
    {
        printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'commit=%s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
        printf 'nproc=%s\n' "$(nproc 2>/dev/null || echo unknown)"
        printf 'loadavg=%s\n' "$(cat /proc/loadavg 2>/dev/null || echo unknown)"
        printf 'requested_workers=%s\n' "$LOAD"
        printf 'kvm='; if [ -c /dev/kvm ]; then echo present; else echo absent; fi
        printf 'qemu=%s\n' "$(qemu-system-x86_64 --version 2>&1 | head -n 1 || echo missing)"
    } > "$path"
}
record_host "$OUT/host-start.txt"

if [ "$PREBUILD" -eq 1 ]; then
    echo "tlb-stress: prebuild $CASE (foreground, cap 420 s)"
    if ! timeout 420 cargo xtask qtest --case "$CASE" --arch x86_64 > "$OUT/prebuild.log" 2>&1; then
        echo "tlb-stress: prebuild failed; see $OUT/prebuild.log" >&2
        exit 2
    fi
    if [ "$RUN_S113" -eq 1 ]; then
        echo "tlb-stress: prebuild S113 images (foreground, cap 600 s)"
        if ! timeout 600 sh tests/smp/memory/run.sh tcg --iterations 1 > "$OUT/prebuild-s113.log" 2>&1; then
            echo "tlb-stress: S113 prebuild failed; see $OUT/prebuild-s113.log" >&2
            exit 2
        fi
    fi
fi

printf 'iteration\tstart_utc\tend_utc\tload1_start\tload5_start\tload15_start\tload1_end\tload5_end\tload15_end\trc\tresult\tduration_s\tlog\n' > "$OUT/results.tsv"
printf 'iteration\ttlb/timeout/stale/FAIL/pass lines\n' > "$OUT/serial-lines.txt"

if [ "$LOAD" -gt 0 ]; then
    sh tests/smp/tlb/load-generator.sh start "$LOAD" "$PIDFILE"
fi
sleep 2
record_host "$OUT/host-load-start.txt"

FAILED=0
iteration=1
while [ "$iteration" -le "$ITERATIONS" ]; do
    started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    load_start=$(cut -d' ' -f1-3 /proc/loadavg)
    begin=$(date +%s)
    log=$OUT/iter-$iteration.log
    stdout=$OUT/iter-$iteration.stdout
    timeout "$QTEST_CAP" cargo xtask qtest --case "$CASE" --arch x86_64 --no-build > "$stdout" 2>&1
    rc=$?
    duration=$(( $(date +%s) - begin ))
    ended=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    load_end=$(cut -d' ' -f1-3 /proc/loadavg)
    if [ "$rc" -eq 0 ]; then result=PASS; else result=FAIL; FAILED=1; fi
    cp "build/qtest/$CASE.log" "$log" 2>/dev/null || : > "$log"
    cp "build/qtest/$CASE.json" "$OUT/iter-$iteration.json" 2>/dev/null || true
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$iteration" "$started" "$ended" \
        "$(printf '%s' "$load_start" | cut -d' ' -f1)" \
        "$(printf '%s' "$load_start" | cut -d' ' -f2)" \
        "$(printf '%s' "$load_start" | cut -d' ' -f3)" \
        "$(printf '%s' "$load_end" | cut -d' ' -f1)" \
        "$(printf '%s' "$load_end" | cut -d' ' -f2)" \
        "$(printf '%s' "$load_end" | cut -d' ' -f3)" \
        "$rc" "$result" "$duration" "$log" >> "$OUT/results.tsv"
    {
        printf 'iteration %s:\n' "$iteration"
        grep -aEi 'tlb|timeout|stale|FAIL|pass' "$log" || true
    } >> "$OUT/serial-lines.txt"
    echo "tlb-stress: iteration $iteration/$ITERATIONS rc=$rc result=$result duration=${duration}s load=$load_start -> $load_end"
    iteration=$((iteration + 1))
done

if [ "$RUN_S113" -eq 1 ]; then
    echo "tlb-stress: S113 TLB reproducer 10/10 (foreground, cap 300 s)"
    begin=$(date +%s)
    timeout "$S113_CAP" sh tests/smp/memory/run.sh tcg --iterations 10 --no-build > "$OUT/s113.log" 2>&1
    rc=$?
    duration=$(( $(date +%s) - begin ))
    printf 's113\trc=%s\tduration_s=%s\tlog=%s\n' "$rc" "$duration" "$OUT/s113.log" >> "$OUT/results.tsv"
    if [ "$rc" -ne 0 ]; then FAILED=1; fi
    echo "tlb-stress: S113 rc=$rc duration=${duration}s"
fi

record_host "$OUT/host-end.txt"
if [ "$LOAD" -gt 0 ]; then
    sh tests/smp/tlb/load-generator.sh stop "$PIDFILE"
    rm -f "$PIDFILE"
fi
sleep 1
record_host "$OUT/host-load-end.txt"

printf 'tlb-stress: out=%s failed=%s\n' "$OUT" "$FAILED"
[ "$FAILED" -eq 0 ]
