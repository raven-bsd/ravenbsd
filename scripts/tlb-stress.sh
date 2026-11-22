#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S120: deterministic TLB stress matrix runner.
#
# Runs the S118 x86 concurrent TLB cases (TCG -smp 4, KVM -smp 2, KVM -smp 4) and the S106
# aarch64 concurrent case N times (default 50). It stops on the first failure, preserves that
# iteration's transcript and JSON, records the host state, and prints min/p50/p90/max plus the
# exact failing case/iteration. It never retries and never relaxes the case expectations.
#
# The exclusive-host preflight is tests/smp/boot/host-idle.sh (S107). A refused host is not a
# sample. Use --no-preflight only for a short smoke, and then the runner records the host state
# itself before the first sample.
#
# usage: scripts/tlb-stress.sh [options]
#   --runs N          iterations per case (default 50; --quick defaults to 1)
#   --case NAME       case to run; repeatable or comma-separated (default: the four TLB cases)
#   --quick           one iteration per requested case
#   --no-preflight    skip host-idle.sh (short smoke only)
#   --max-load LOAD   host-idle max 1-minute load (default host-idle's max(2, nproc/4))
#   --wait SECONDS    wait up to SECONDS for an idle host (default 0: refuse)
#   --out DIR         output directory (default build/tlb-stress)
#   -h, --help        this help
#
# Outputs under DIR:
#   host-idle.txt            host-idle.sh state/decision (preflight mode)
#   host-state-before.txt    nproc/load/QEMU/qtest state before the first sample
#   host-state-after.txt     the same state after the last sample
#   results.tsv              case/arch/iteration/rc/duration_ms/json/log for every sample
#   summary.txt              runs/fail/min/p50/p90/max per case
#   <case>/iter-N.log        full qtest transcript for that iteration
#   <case>/iter-N.json       the qtest JSON for that iteration
#
# Exit: 0 all requested samples passed; 1 first failure preserved; 2 refused/bad invocation.

set -u
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

RUNS=50
RUNS_SET=0
QUICK=0
PREFLIGHT=1
OUT=build/tlb-stress
REQUESTED=
MAX_LOAD=
WAIT_IDLE=0

DEFAULT_CASES="smp-tlb-x86-concurrent smp-tlb-x86-concurrent-kvm2 smp-tlb-x86-concurrent-kvm4 smp-tlb-aarch64-concurrent"

usage() {
    sed -n '6,35p' "$0" | sed 's/^# \{0,1\}//'
}

add_cases() {
    old_ifs=$IFS
    IFS=,
    for requested in $1; do
        [ -n "$requested" ] || continue
        REQUESTED="${REQUESTED}${REQUESTED:+ }$requested"
    done
    IFS=$old_ifs
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --runs)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --runs needs a value" >&2; exit 2; }
            RUNS=$2
            RUNS_SET=1
            shift 2
            ;;
        --case)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --case needs a value" >&2; exit 2; }
            add_cases "$2"
            shift 2
            ;;
        --quick)
            QUICK=1
            shift
            ;;
        --no-preflight)
            PREFLIGHT=0
            shift
            ;;
        --max-load)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --max-load needs a value" >&2; exit 2; }
            MAX_LOAD=$2
            shift 2
            ;;
        --wait)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --wait needs a value" >&2; exit 2; }
            WAIT_IDLE=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --out needs a value" >&2; exit 2; }
            OUT=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "tlb-stress: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ "$QUICK" -eq 1 ] && [ "$RUNS_SET" -eq 0 ]; then
    RUNS=1
fi
case "$RUNS" in
    ''|*[!0-9]*)
        echo "tlb-stress: --runs must be a positive integer" >&2
        exit 2
        ;;
esac
if [ "$RUNS" -lt 1 ]; then
    echo "tlb-stress: --runs must be at least 1" >&2
    exit 2
fi

arch_for() {
    case "$1" in
        smp-tlb-x86-concurrent|smp-tlb-x86-concurrent-kvm2|smp-tlb-x86-concurrent-kvm4)
            echo x86_64
            ;;
        smp-tlb-aarch64-concurrent)
            echo aarch64
            ;;
        *)
            return 1
            ;;
    esac
}

is_kvm_case() {
    case "$1" in
        *-kvm2|*-kvm4) return 0 ;;
        *) return 1 ;;
    esac
}

[ -n "$REQUESTED" ] || REQUESTED=$DEFAULT_CASES

NEED_X86=0
NEED_AARCH64=0
NEED_KVM=0
for case_name in $REQUESTED; do
    arch=$(arch_for "$case_name") || {
        echo "tlb-stress: unknown case: $case_name" >&2
        exit 2
    }
    case "$arch" in
        x86_64) NEED_X86=1 ;;
        aarch64) NEED_AARCH64=1 ;;
    esac
    if is_kvm_case "$case_name"; then
        NEED_KVM=1
    fi
done

if [ -e "$OUT/results.tsv" ]; then
    echo "tlb-stress: refusing to mix samples: $OUT/results.tsv exists; pass a fresh --out" >&2
    exit 2
fi
mkdir -p "$OUT" || exit 2

record_host() {
    path=$1
    context=$2
    {
        printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'context=%s\n' "$context"
        printf 'nproc=%s\n' "$(nproc 2>/dev/null || echo unknown)"
        printf 'loadavg=%s\n' "$(cat /proc/loadavg 2>/dev/null || echo unknown)"
        printf 'git=%s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
        if [ -r /dev/kvm ]; then
            printf 'kvm=available\n'
        else
            printf 'kvm=unavailable\n'
        fi
        printf 'qemu_x86_64=%s\n' "$(qemu-system-x86_64 --version 2>&1 | head -n 1 || echo missing)"
        printf 'qemu_aarch64=%s\n' "$(qemu-system-aarch64 --version 2>&1 | head -n 1 || echo missing)"
        printf 'qemu_processes:\n%s\n' "$(pgrep -fa '^qemu-system-' 2>/dev/null || echo none)"
        printf 'qtest_processes:\n%s\n' "$(pgrep -fa 'xtask qtest' 2>/dev/null || echo none)"
    } > "$path" 2>/dev/null || true
}

if [ "$PREFLIGHT" -eq 1 ]; then
    if [ ! -f tests/smp/boot/host-idle.sh ]; then
        echo "tlb-stress: tests/smp/boot/host-idle.sh is missing; use --no-preflight for a smoke" >&2
        exit 2
    fi
    set -- --context tlb-stress --state-file "$OUT/host-idle.txt" --wait "$WAIT_IDLE"
    if [ -n "$MAX_LOAD" ]; then
        set -- "$@" --max-load "$MAX_LOAD"
    fi
    if sh tests/smp/boot/host-idle.sh "$@"; then
        cp "$OUT/host-idle.txt" "$OUT/host-state-before.txt" 2>/dev/null || true
    else
        refused=$?
        record_host "$OUT/host-state-before.txt" "preflight-refused"
        echo "tlb-stress: host refused preflight (exit $refused); no samples run" >&2
        exit 2
    fi
else
    record_host "$OUT/host-state-before.txt" "no-preflight"
fi

if [ "$NEED_X86" -eq 1 ] && ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "tlb-stress: qemu-system-x86_64 is missing; no samples run" >&2
    exit 2
fi
if [ "$NEED_AARCH64" -eq 1 ] && ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
    echo "tlb-stress: qemu-system-aarch64 is missing; no samples run" >&2
    exit 2
fi
if [ "$NEED_KVM" -eq 1 ]; then
    if [ ! -r /dev/kvm ]; then
        echo "tlb-stress: /dev/kvm is not readable; refusing KVM cases (no samples)" >&2
        exit 2
    fi
    if ! qemu-system-x86_64 -accel help 2>&1 | grep -qx 'kvm'; then
        echo "tlb-stress: QEMU has no KVM accelerator; refusing KVM cases (no samples)" >&2
        exit 2
    fi
fi

printf 'tlb-stress: commit=%s runs=%s cases=%s preflight=%s out=%s\n' \
    "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" "$RUNS" "$REQUESTED" \
    "$([ "$PREFLIGHT" -eq 1 ] && echo host-idle || echo skipped)" "$OUT"

: > "$OUT/results.tsv"
printf 'case\tarch\titeration\trc\tduration_ms\tjson\tlog\n' >> "$OUT/results.tsv"

FAILED=0
FAILED_CASE=
FAILED_ARCH=
FAILED_ITER=
RAN_CASES=0

for case_name in $REQUESTED; do
    arch=$(arch_for "$case_name")
    case_dir="$OUT/$case_name"
    mkdir -p "$case_dir" || exit 2
    RAN_CASES=$((RAN_CASES + 1))
    iteration=1
    while [ "$iteration" -le "$RUNS" ]; do
        log="$case_dir/iter-$iteration.log"
        json="$case_dir/iter-$iteration.json"
        started=$(date +%s)
        if cargo xtask qtest --case "$case_name" --arch "$arch" > "$log" 2>&1; then
            rc=0
        else
            rc=$?
        fi
        source_json="build/qtest/$case_name.json"
        if [ -f "$source_json" ]; then
            cp "$source_json" "$json"
        else
            : > "$json"
        fi
        duration=$(sed -n 's/.*"duration_ms": \([0-9][0-9]*\).*/\1/p' "$json" 2>/dev/null | head -n 1)
        [ -n "${duration:-}" ] || duration=$(( ($(date +%s) - started) * 1000 ))
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$case_name" "$arch" "$iteration" "$rc" "$duration" "$json" "$log" >> "$OUT/results.tsv"
        if [ "$rc" -ne 0 ]; then
            FAILED=1
            FAILED_CASE=$case_name
            FAILED_ARCH=$arch
            FAILED_ITER=$iteration
            break 2
        fi
        iteration=$((iteration + 1))
    done
done

record_host "$OUT/host-state-after.txt" after

summary="$OUT/summary.txt"
{
    printf 'case\tarch\truns\tfail\tmin_ms\tp50_ms\tp90_ms\tmax_ms\n'
    awk -F '	' 'NR > 1 { print $1 " " $2 }' "$OUT/results.tsv" | sort -u |
        while read -r summary_case summary_arch; do
            durations=$(awk -F '	' -v c="$summary_case" -v a="$summary_arch" \
                'NR > 1 && $1 == c && $2 == a { print $5 }' "$OUT/results.tsv" | sort -n)
            samples=$(printf '%s\n' "$durations" | grep -c . || true)
            if [ "$samples" -eq 0 ]; then
                printf '%s\t%s\t0\t0\t-\t-\t-\t-\n' "$summary_case" "$summary_arch"
                continue
            fi
            failures=$(awk -F '	' -v c="$summary_case" -v a="$summary_arch" \
                'NR > 1 && $1 == c && $2 == a && $4 != 0 { f++ } END { print f + 0 }' \
                "$OUT/results.tsv")
            stats=$(printf '%s\n' "$durations" | awk -v n="$samples" '
                { value[NR] = $1 }
                END {
                    p50 = value[int((n + 1) / 2)];
                    idx = int((9 * n + 9) / 10);
                    if (idx > n) idx = n;
                    if (idx < 1) idx = 1;
                    printf "%s\t%s\t%s\t%s", value[1], p50, value[idx], value[n];
                }')
            printf '%s\t%s\t%s\t%s\t%s\n' \
                "$summary_case" "$summary_arch" "$samples" "$failures" "$stats"
        done
} > "$summary"

cat "$summary"

if [ "$FAILED" -eq 1 ]; then
    echo "tlb-stress: FAIL case=$FAILED_CASE arch=$FAILED_ARCH iteration=$FAILED_ITER runs=$RUNS (no retry)" >&2
    echo "tlb-stress: log=$OUT/$FAILED_CASE/iter-$FAILED_ITER.log" >&2
    echo "tlb-stress: json=$OUT/$FAILED_CASE/iter-$FAILED_ITER.json" >&2
    failed_log="$OUT/$FAILED_CASE/iter-$FAILED_ITER.log"
    if grep -q -- '--- forbidden hits ---' "$failed_log" 2>/dev/null; then
        sed -n '/--- forbidden hits ---/,$p' "$failed_log" | head -n 20 >&2
    else
        tail -n 20 "$failed_log" >&2
    fi
    exit 1
fi

echo "tlb-stress: PASS cases=$RAN_CASES runs=$RUNS"
exit 0
