#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# TLB stress harness.
#
# S120 matrix mode (default): runs the S118 x86 concurrent TLB cases (TCG -smp 4,
# KVM -smp 2, KVM -smp 4) and the S106 aarch64 concurrent case N times (default 50).
# It stops on the first failure, preserves that iteration's transcript and JSON,
# records the host state, and prints min/p50/p90/max plus the exact failing
# case/iteration. It never retries and never relaxes the case expectations.
#
# K376 gate mode (`--load`): drives the existing `posix-fs-smp` TCG -smp 4 case for N
# iterations under a deterministic bounded CPU load, recording per-iteration start/end,
# load 1/5/15, exit code, duration, serial log and every tlb/timeout/stale/FAIL/pass line.
# It also runs the S113 TLB reproducer 10/10 separately. No retry hides a failure; the
# run completes the requested rows and exits 1 if any row failed.
#
# The exclusive-host preflight is tests/smp/boot/host-idle.sh (S107). A refused host is
# not a sample. Use --no-preflight only for a short smoke, and then the runner records
# the host state itself before the first sample. K376 mode has its own low-load guard
# and does not use host-idle.
#
# Exit: 0 all requested samples passed; 1 first failure preserved; 2 refused/bad
# invocation (including a K376 low-load control that refuses a busy host).

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
OUT_SET=0
REQUESTED=
MAX_LOAD=
WAIT_IDLE=0
K376=0
K376_EXTRA=0
LOAD=100
K376_ITERATIONS=20
K376_CASE=
PREBUILD=1
RUN_S113=1
S113_SET=0
MAX_HOST_LOAD=20

DEFAULT_CASES="smp-tlb-x86-concurrent smp-tlb-x86-concurrent-kvm2 smp-tlb-x86-concurrent-kvm4 smp-tlb-aarch64-concurrent"

usage() {
    cat <<'EOF'
usage: scripts/tlb-stress.sh [options]

S120 matrix mode (default):
  --runs N              iterations per case (default 50; --quick defaults to 1)
  --case NAME           case to run; repeatable or comma-separated (default: four cases)
  --quick               one iteration per requested case
  --no-preflight        skip host-idle.sh (short smoke only)
  --max-load LOAD       host-idle max 1-minute load (default host-idle's max(2, nproc/4))
  --wait SECONDS        wait up to SECONDS for an idle host (default 0: refuse)
  --out DIR             output directory (default build/tlb-stress)
  -h, --help            this help

K376 load-matched gate mode (enabled by --load):
  --load N              busy workers (default 100; 0 is the low-load control)
  --iterations N        qtest iterations (default 20)
  --case NAME           qtest case (default posix-fs-smp; TCG -smp 4)
  --no-prebuild         reuse the existing qtest image (the prebuild is otherwise run once)
  --no-s113             skip the separate S113 TLB 10/10 run (not recommended)
  --s113                force the S113 run (default on except for --load 0)
  --max-host-load L     max host load for the --load 0 control (default 20)
  --out DIR             output directory (default build/tlb-stress/<UTC>)

S120 outputs under DIR: host-idle.txt, host-state-before/after.txt, results.tsv,
summary.txt, <case>/iter-N.log and <case>/iter-N.json.
K376 outputs under DIR: host-start.txt, host-load-start.txt, host-end.txt,
results.tsv, serial-lines.txt, iter-N.log, iter-N.json, prebuild.log and s113.log.
EOF
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
            K376_CASE=$2
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
            OUT_SET=1
            shift 2
            ;;
        --load)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --load needs a value" >&2; exit 2; }
            K376=1
            LOAD=$2
            shift 2
            ;;
        --iterations)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --iterations needs a value" >&2; exit 2; }
            K376_ITERATIONS=$2
            K376_EXTRA=1
            shift 2
            ;;
        --max-host-load)
            [ "$#" -ge 2 ] || { echo "tlb-stress: --max-host-load needs a value" >&2; exit 2; }
            MAX_HOST_LOAD=$2
            K376_EXTRA=1
            shift 2
            ;;
        --no-prebuild)
            PREBUILD=0
            K376_EXTRA=1
            shift
            ;;
        --no-s113)
            RUN_S113=0
            S113_SET=1
            K376_EXTRA=1
            shift
            ;;
        --s113)
            RUN_S113=1
            S113_SET=1
            K376_EXTRA=1
            shift
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

if [ "$K376_EXTRA" -eq 1 ] && [ "$K376" -eq 0 ]; then
    echo "tlb-stress: --iterations/--no-prebuild/--no-s113/--s113/--max-host-load require --load" >&2
    exit 2
fi

if [ "$K376" -eq 1 ]; then
    case "$LOAD" in
        ''|*[!0-9]*)
            echo "tlb-stress: --load must be a non-negative integer" >&2
            exit 2
            ;;
    esac
    case "$K376_ITERATIONS" in
        ''|*[!0-9]*)
            echo "tlb-stress: --iterations must be a positive integer" >&2
            exit 2
            ;;
    esac
    if [ "$K376_ITERATIONS" -lt 1 ]; then
        echo "tlb-stress: --iterations must be at least 1" >&2
        exit 2
    fi
    [ -n "$K376_CASE" ] || K376_CASE=posix-fs-smp
    case "$K376_CASE" in
        *,*)
            echo "tlb-stress: K376 mode runs one case; --case must not be a list" >&2
            exit 2
            ;;
    esac
    if [ "$OUT_SET" -eq 0 ]; then
        OUT=build/tlb-stress/$(date -u +%Y%m%dT%H%M%SZ)
    fi
    # The low-load control is the quick acceptance path; the S113 10/10 run belongs to
    # the load-matched run unless explicitly requested.
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
    if [ ! -f tests/smp/tlb/load-generator.sh ]; then
        echo "tlb-stress: tests/smp/tlb/load-generator.sh is missing; no samples run" >&2
        exit 2
    fi
    if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
        echo "tlb-stress: qemu-system-x86_64 is missing; no samples run" >&2
        exit 2
    fi
    if [ -e "$OUT/results.tsv" ]; then
        echo "tlb-stress: refusing to mix samples: $OUT/results.tsv exists; pass a fresh --out" >&2
        exit 2
    fi
    mkdir -p "$OUT" || exit 2

    # Per-iteration qtest cap: the case's own timeout_secs is 300; 420 s leaves the
    # measured boot/build margin and lets the case fail itself instead of the wrapper.
    QTEST_CAP=420
    S113_CAP=300
    PIDFILE=$OUT/load.pids
    trap 'sh tests/smp/tlb/load-generator.sh stop "$PIDFILE" >/dev/null 2>&1 || true' EXIT INT TERM

    record_host "$OUT/host-start.txt" "k376-start"
    printf 'tlb-stress: K376 load=%s iterations=%s case=%s prebuild=%s s113=%s out=%s\n' \
        "$LOAD" "$K376_ITERATIONS" "$K376_CASE" "$PREBUILD" "$RUN_S113" "$OUT"

    if [ "$PREBUILD" -eq 1 ]; then
        echo "tlb-stress: prebuild $K376_CASE (foreground, cap 420 s)"
        if ! timeout 420 cargo xtask qtest --case "$K376_CASE" --arch x86_64 > "$OUT/prebuild.log" 2>&1; then
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
    record_host "$OUT/host-load-start.txt" "k376-load-start"

    FAILED=0
    iteration=1
    while [ "$iteration" -le "$K376_ITERATIONS" ]; do
        started=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        load_start=$(cut -d' ' -f1-3 /proc/loadavg)
        begin=$(date +%s)
        log=$OUT/iter-$iteration.log
        stdout=$OUT/iter-$iteration.stdout
        timeout "$QTEST_CAP" cargo xtask qtest --case "$K376_CASE" --arch x86_64 --no-build > "$stdout" 2>&1
        rc=$?
        duration=$(( $(date +%s) - begin ))
        ended=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        load_end=$(cut -d' ' -f1-3 /proc/loadavg)
        if [ "$rc" -eq 0 ]; then result=PASS; else result=FAIL; FAILED=1; fi
        cp "build/qtest/$K376_CASE.log" "$log" 2>/dev/null || : > "$log"
        cp "build/qtest/$K376_CASE.json" "$OUT/iter-$iteration.json" 2>/dev/null || true
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
        echo "tlb-stress: iteration $iteration/$K376_ITERATIONS rc=$rc result=$result duration=${duration}s load=$load_start -> $load_end"
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

    record_host "$OUT/host-end.txt" "k376-end"
    if [ "$LOAD" -gt 0 ]; then
        sh tests/smp/tlb/load-generator.sh stop "$PIDFILE"
        rm -f "$PIDFILE"
    fi
    sleep 1
    record_host "$OUT/host-load-end.txt" "k376-load-end"

    printf 'tlb-stress: out=%s failed=%s\n' "$OUT" "$FAILED"
    [ "$FAILED" -eq 0 ]
    exit $?
fi

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
