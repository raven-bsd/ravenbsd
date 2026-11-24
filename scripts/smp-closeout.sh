#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S126: deterministic SMP close-out matrix runner.
#
# Runs the POSIX/stdio/VFS cases on TCG -smp 4 and KVM -smp 1/2/4, in that order, once per
# cell. It stops on the first failure, preserves that cell's qtest JSON, serial log and stdout,
# records the host state, and prints min/p50/p90/max. It never retries and never relaxes a case
# expectation.
#
# The exclusive-host preflight is tests/smp/boot/host-idle.sh (S107): a refused host is zero
# samples. --no-preflight exists for a short smoke on a shared host; it records the host state
# itself and is not release evidence.
#
# usage: scripts/smp-closeout.sh [options]
#   --case NAME       case to run; repeatable or comma-separated (default: the five cases below)
#   --mode NAME       mode to run (tcg4,kvm1,kvm2,kvm4); repeatable/comma-separated
#   --quick           TCG -smp 4 column only (smoke; not release evidence)
#   --no-preflight    skip tests/smp/boot/host-idle.sh (short smoke only)
#   --max-load LOAD   preflight maximum 1-minute load average (host-idle.sh default otherwise)
#   --wait-for-idle S wait up to S seconds for an idle host (foreground poll, default 0)
#   --cell-timeout S  cap override for a SINGLE-CELL invocation only (e.g., a smoke). There is
#                     no blanket default; every matrix cell uses the table below. A cell over its
#                     cap is a timeout failure with the last 20 serial lines; it is never retried.
#   --out DIR         output root (default build/smp-closeout/<UTC>)
#   -h, --help        this help
#
# Per-cell caps in seconds (expected runtime + proportional margin; S126 rev 59):
#   case                 tcg4  kvm1  kvm2  kvm4
#   posix-shell            40    20    20    20
#   posix-util-system      35    25    25    25
#   libc-threads          170    30    30    30
#   smp-libc-concurrency   70    30    30    30
#   posix-fs-smp          140    40    40    40
#
# Cases (base name -> qtest case):
#   tcg4  <case>            KVM1  <case>-kvm1
#   KVM2  <case>-kvm2       KVM4  <case>-kvm4
#
# Default cases: posix-shell posix-util-system libc-threads smp-libc-concurrency posix-fs-smp
#
# Outputs under DIR:
#   host-idle.txt            preflight state/decision
#   host-state-before.txt    host state before the first sample (--no-preflight)
#   host-state-after.txt     host state after the last sample
#   libc-pack.log            sysroot/libc build log
#   results.tsv              mode/case/qtest_case/rc/result/duration_ms/log/cell_cap_s/cell_timeout
#   summary.txt              commit, host, per-mode and overall min/p50/p90/max
#   <mode>/<qtest>.json      qtest JSON for that cell
#   <mode>/<qtest>.log       serial log for that cell
#   <mode>/<qtest>.out       qtest stdout for that cell
#   FAILED                   mode/case/rc of the first failure
#
# Exit: 0 all cells passed; 1 first failure preserved; 2 refused/bad invocation.

set -u
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

DEFAULT_CASES="posix-shell posix-util-system libc-threads smp-libc-concurrency posix-fs-smp"
ALL_MODES="tcg4 kvm1 kvm2 kvm4"
MODES=
CASES=
QUICK=0
PREFLIGHT=1
OUT=build/smp-closeout/$(date -u +%Y%m%dT%H%M%SZ)
MAX_LOAD=
WAIT_IDLE=0
CELL_TIMEOUT=

usage() {
    sed -n '6,45p' "$0" | sed 's/^# \{0,1\}//'
}

add_cases() {
    old_ifs=$IFS
    IFS=,
    for requested in $1; do
        [ -n "$requested" ] || continue
        CASES="${CASES}${CASES:+ }$requested"
    done
    IFS=$old_ifs
}

add_modes() {
    old_ifs=$IFS
    IFS=,
    for requested in $1; do
        [ -n "$requested" ] || continue
        case $requested in
            tcg4 | kvm1 | kvm2 | kvm4) ;;
            *)
                echo "smp-closeout: unknown mode $requested (tcg4,kvm1,kvm2,kvm4)" >&2
                exit 2
                ;;
        esac
        MODES="${MODES}${MODES:+ }$requested"
    done
    IFS=$old_ifs
}

# Per-cell wall-clock caps in seconds. S126 rev 59: expected runtime plus a proportional margin
# measured from the round-3 runs. There is no blanket default; a missing entry needs an explicit
# single-cell --cell-timeout.
cell_cap() {
    case "$1:$2" in
        tcg4:posix-shell) printf '40' ;;
        tcg4:posix-util-system) printf '35' ;;
        tcg4:libc-threads) printf '170' ;;
        tcg4:smp-libc-concurrency) printf '70' ;;
        tcg4:posix-fs-smp) printf '140' ;;
        kvm1:posix-shell | kvm2:posix-shell | kvm4:posix-shell) printf '20' ;;
        kvm1:posix-util-system | kvm2:posix-util-system | kvm4:posix-util-system) printf '25' ;;
        kvm1:libc-threads | kvm2:libc-threads | kvm4:libc-threads) printf '30' ;;
        kvm1:smp-libc-concurrency | kvm2:smp-libc-concurrency | kvm4:smp-libc-concurrency) printf '30' ;;
        kvm1:posix-fs-smp | kvm2:posix-fs-smp | kvm4:posix-fs-smp) printf '40' ;;
        *) printf '' ;;
    esac
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --case)
            [ "$#" -ge 2 ] || { echo "smp-closeout: --case needs a value" >&2; exit 2; }
            add_cases "$2"
            shift 2
            ;;
        --mode)
            [ "$#" -ge 2 ] || { echo "smp-closeout: --mode needs a value" >&2; exit 2; }
            add_modes "$2"
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
            [ "$#" -ge 2 ] || { echo "smp-closeout: --max-load needs a value" >&2; exit 2; }
            MAX_LOAD=$2
            shift 2
            ;;
        --wait-for-idle)
            [ "$#" -ge 2 ] || { echo "smp-closeout: --wait-for-idle needs a value" >&2; exit 2; }
            WAIT_IDLE=$2
            shift 2
            ;;
        --cell-timeout)
            [ "$#" -ge 2 ] || { echo "smp-closeout: --cell-timeout needs a value" >&2; exit 2; }
            CELL_TIMEOUT=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { echo "smp-closeout: --out needs a value" >&2; exit 2; }
            OUT=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "smp-closeout: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

[ -n "$CASES" ] || CASES=$DEFAULT_CASES
case $CELL_TIMEOUT in
    '') ;;
    0 | *[!0-9]*)
        echo "smp-closeout: --cell-timeout must be a positive integer" >&2
        exit 2
        ;;
esac
if [ "$QUICK" -eq 1 ]; then
    MODES=tcg4
elif [ -z "$MODES" ]; then
    MODES=$ALL_MODES
fi

# The caller passes base TCG case names; the runner appends -kvmN. Reject a pre-mapped name so
# the matrix cannot silently run the wrong case.
for case_name in $CASES; do
    case $case_name in
        *-kvm1 | *-kvm2 | *-kvm4)
            echo "smp-closeout: pass the base case name, not $case_name" >&2
            exit 2
            ;;
    esac
done

# Validate every requested cell before the preflight, so a typo is not reported as a busy host.
selected_cells=0
for mode in $MODES; do
    for case_name in $CASES; do
        if [ "$mode" = tcg4 ]; then
            qtest_case=$case_name
        else
            qtest_case=$case_name-$mode
        fi
        if [ ! -f "tests/qtest/cases/$qtest_case.toml" ]; then
            echo "smp-closeout: missing case tests/qtest/cases/$qtest_case.toml" >&2
            exit 2
        fi
        selected_cells=$((selected_cells + 1))
        if [ -z "$CELL_TIMEOUT" ]; then
            cap=$(cell_cap "$mode" "$case_name")
            if [ -z "$cap" ]; then
                echo "smp-closeout: no cap table entry for mode=$mode case=$case_name; use --cell-timeout N for a single-cell run" >&2
                exit 2
            fi
        fi
    done
done
if [ -n "$CELL_TIMEOUT" ] && [ "$selected_cells" -ne 1 ]; then
    echo "smp-closeout: --cell-timeout overrides one cell; this invocation selects $selected_cells" >&2
    exit 2
fi

mkdir -p "$OUT" || exit 2

record_host() {
    destination=$1
    {
        printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'commit=%s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
        printf 'branch=%s\n' "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
        printf 'dirty=%s\n' "$(git status --porcelain 2>/dev/null | wc -l)"
        printf 'nproc=%s\n' "$(nproc 2>/dev/null || echo unknown)"
        printf 'loadavg=%s\n' "$(cat /proc/loadavg 2>/dev/null || echo unknown)"
        printf 'qemu:\n%s\n' "$(pgrep -fa '^(qemu-system-|/.*qemu-system-)' 2>/dev/null || echo none)"
        printf 'jobs:\n%s\n' "$(pgrep -fa '^(target/(debug|release)/xtask|target/(debug|release)/raven-qtest|/.*/raven-qtest|cargo (xtask|build|test|run)|rustc|/.*rustc)' 2>/dev/null || echo none)"
        printf 'kvm='; ls -l /dev/kvm 2>&1 || true
        printf 'qemu_version='; qemu-system-x86_64 --version 2>&1 | head -n 1 || true
        printf 'free:\n'; free -h 2>/dev/null | head -n 2 || true
    } > "$destination" 2>/dev/null || true
}

if [ "$PREFLIGHT" -eq 1 ]; then
    echo "smp-closeout: exclusive preflight (tests/smp/boot/host-idle.sh)"
    set -- --context smp-closeout --state-file "$OUT/host-idle.txt"
    [ -n "$MAX_LOAD" ] && set -- "$@" --max-load "$MAX_LOAD"
    [ "$WAIT_IDLE" -gt 0 ] && set -- "$@" --wait "$WAIT_IDLE"
    if ! sh tests/smp/boot/host-idle.sh "$@"; then
        echo "smp-closeout: exclusive preflight refused the host; zero samples run" >&2
        echo "smp-closeout: state=$OUT/host-idle.txt" >&2
        exit 2
    fi
else
    echo "smp-closeout: --no-preflight (smoke only; host state is recorded, not gated)"
    record_host "$OUT/host-state-before.txt"
fi

echo "smp-closeout: cargo xtask libc-pack --arch x86_64"
if ! cargo xtask libc-pack --arch x86_64 > "$OUT/libc-pack.log" 2>&1; then
    echo "smp-closeout: libc-pack failed; see $OUT/libc-pack.log" >&2
    exit 2
fi

printf 'mode\tcase\tqtest_case\trc\tresult\tduration_ms\tlog\tcell_cap_s\tcell_timeout\n' > "$OUT/results.tsv"
printf 'mode\tqtest_case\tload1\tqemu\tutc\n' > "$OUT/host-per-cell.tsv"

# One telemetry row: UTC, load1, and the count of QEMU, xtask, cargo and rustc processes.
# The counts include this cell's own processes (the cell's qtest/xtask), so overlap with another
# lane shows as qemu>=2 or xtask>=2 (and cargo/rustc outside the cell's build phase).
count_matches() {
    matches=$(pgrep -c "$@" 2>/dev/null || true)
    [ -n "$matches" ] || matches=0
    printf '%s' "$matches"
}

telemetry_row() {
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        "$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo unknown)" \
        "$(count_matches -f '^qemu-system-')" \
        "$(count_matches -f 'target/(debug|release)/xtask')" \
        "$(count_matches -f 'cargo (xtask|build|test|run|nextest)')" \
        "$(count_matches -x rustc)"
}

run_cell() {
    mode=$1
    case_name=$2
    qtest_case=$3
    directory=$OUT/$mode
    mkdir -p "$directory" || return 2

    if [ -n "$CELL_TIMEOUT" ]; then
        cell_cap_s=$CELL_TIMEOUT
    else
        cell_cap_s=$(cell_cap "$mode" "$case_name")
    fi

    printf '%s\t%s\t%s\t%s\t%s\n' \
        "$mode" "$qtest_case" \
        "$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo unknown)" \
        "$(pgrep -c -f '^qemu-system-' 2>/dev/null || true)" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$OUT/host-per-cell.tsv"

    telemetry=$directory/$qtest_case.telemetry.tsv
    printf 'utc\tload1\tqemu\txtask\tcargo\trustc\n' > "$telemetry"
    telemetry_row >> "$telemetry"
    (
        while :; do
            sleep 5
            telemetry_row >> "$telemetry"
        done
    ) &
    sampler=$!

    # A stale harness JSON/log from an earlier run must not be mistaken for this cell's result.
    rm -f "build/qtest/$qtest_case.log" "build/qtest/$qtest_case.json"

    cap_flag=$directory/$qtest_case.cap-fired
    cap_stop=$directory/$qtest_case.cap-stop
    cap_log=$directory/$qtest_case.cap.log
    rm -f "$cap_flag" "$cap_stop" "$cap_log"

    # Per-cell cap: kill only this cell's QEMU, so the harness sees the exit and still writes its
    # serial log/JSON. If the harness does not exit, the hard timeout below kills it at cap+5 s.
    (
        slept=0
        while [ "$slept" -lt "$cell_cap_s" ]; do
            [ -e "$cap_stop" ] && exit 0
            sleep 1
            slept=$((slept + 1))
        done
        [ -e "$cap_stop" ] && exit 0
        pids=$(pgrep -f "file=build/qtest/$qtest_case.img" 2>/dev/null || true)
        if [ -n "$pids" ]; then
            : > "$cap_flag"
            {
                printf 'smp-closeout: cell cap %ss reached for %s\n' "$cell_cap_s" "$qtest_case"
                printf 'terminating QEMU pids: %s\n' "$pids"
            } >> "$cap_log"
            # Word splitting is intended: one TERM per QEMU pid of this cell.
            kill -TERM $pids 2>/dev/null || true
        fi
    ) &
    cap_pid=$!

    start_ns=$(date +%s%N)
    timeout --kill-after=5 "$((cell_cap_s + 5))s" \
        cargo xtask qtest --case "$qtest_case" --arch x86_64 \
        > "$directory/$qtest_case.out" 2>&1
    rc=$?
    end_ns=$(date +%s%N)
    wall_ms=$(( (end_ns - start_ns) / 1000000 ))

    : > "$cap_stop"
    kill "$cap_pid" 2>/dev/null || true
    wait "$cap_pid" 2>/dev/null || true

    kill "$sampler" 2>/dev/null || true
    wait "$sampler" 2>/dev/null || true
    telemetry_row >> "$telemetry"

    # Preserve the harness artifacts before anything else can overwrite them.
    cp "build/qtest/$qtest_case.log" "$directory/$qtest_case.log" 2>/dev/null || true
    cp "build/qtest/$qtest_case.json" "$directory/$qtest_case.json" 2>/dev/null || true

    result=missing
    duration_ms=$wall_ms
    cell_timeout=0
    if [ -e "$cap_flag" ] || [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
        cell_timeout=1
        rc=1
    fi
    if [ -f "$directory/$qtest_case.json" ]; then
        parsed=$(sed -n 's/.*"result": "\([a-z]*\)".*/\1/p' "$directory/$qtest_case.json" | head -n 1)
        [ -n "$parsed" ] && result=$parsed
        parsed=$(sed -n 's/.*"duration_ms": *\([0-9][0-9]*\).*/\1/p' "$directory/$qtest_case.json" | head -n 1)
        [ -n "$parsed" ] && duration_ms=$parsed
    fi
    if [ "$cell_timeout" -eq 1 ]; then
        result=timeout
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$mode" "$case_name" "$qtest_case" "$rc" "$result" "$duration_ms" \
        "$directory/$qtest_case.log" "$cell_cap_s" "$cell_timeout" >> "$OUT/results.tsv"

    if [ "$rc" -ne 0 ]; then
        {
            printf 'mode=%s\n' "$mode"
            printf 'case=%s\n' "$case_name"
            printf 'qtest_case=%s\n' "$qtest_case"
            printf 'rc=%s\n' "$rc"
            printf 'result=%s\n' "$result"
            printf 'cell_timeout=%s\n' "$cell_timeout"
            printf 'cell_cap_s=%s\n' "$cell_cap_s"
            printf 'log=%s\n' "$directory/$qtest_case.log"
            printf 'json=%s\n' "$directory/$qtest_case.json"
            printf 'out=%s\n' "$directory/$qtest_case.out"
        } > "$OUT/FAILED"
        echo "smp-closeout: FAIL mode=$mode case=$qtest_case rc=$rc result=$result cell_timeout=$cell_timeout" >&2
        if [ "$cell_timeout" -eq 1 ]; then
            echo "--- serial (last 20 lines) ---" >&2
            tail -n 20 "$directory/$qtest_case.log" >&2 || tail -n 20 "$directory/$qtest_case.out" >&2 || true
        else
            echo "--- $directory/$qtest_case.out (tail) ---" >&2
            tail -n 20 "$directory/$qtest_case.out" >&2 || true
            echo "--- $directory/$qtest_case.log (tail) ---" >&2
            tail -n 20 "$directory/$qtest_case.log" >&2 || true
        fi
        return 1
    fi
    return 0
}

cells=0
for mode in $MODES; do
    for case_name in $CASES; do
        if [ "$mode" = tcg4 ]; then
            qtest_case=$case_name
        else
            qtest_case=$case_name-$mode
        fi
        echo "smp-closeout: [$((cells + 1))] mode=$mode case=$qtest_case"
        if ! run_cell "$mode" "$case_name" "$qtest_case"; then
            record_host "$OUT/host-state-after.txt"
            echo "smp-closeout: stopped on the first failure; no retry" >&2
            exit 1
        fi
        cells=$((cells + 1))
    done
done

record_host "$OUT/host-state-after.txt"

percentiles() {
    label=$1
    values=$2
    if [ ! -s "$values" ]; then
        printf '%s: n=0\n' "$label"
        return
    fi
    sort -n "$values" | awk -v label="$label" '
        { a[NR] = $1 }
        END {
            n = NR
            if (n == 0) { printf "%s: n=0\n", label; exit }
            p50 = a[int((n + 1) / 2)]
            p90 = a[int((n * 9 + 9) / 10)]
            printf "%s: n=%d min=%d p50=%d p90=%d max=%d (ms)\n", label, n, a[1], p50, p90, a[n]
        }'
}

awk -F'\t' 'NR > 1 && $4 == 0 { print $6 }' "$OUT/results.tsv" > "$OUT/durations.all"

{
    printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'commit=%s\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
    printf 'branch=%s\n' "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
    printf 'preflight=%s\n' "$([ "$PREFLIGHT" -eq 1 ] && echo exclusive || echo skipped)"
    printf 'max_load=%s\n' "${MAX_LOAD:-host-idle-default}"
    printf 'wait_for_idle=%s\n' "$WAIT_IDLE"
    printf 'cell_timeout_override=%s\n' "${CELL_TIMEOUT:-per-case-table}"
    printf 'quick=%s\n' "$QUICK"
    printf 'cells=%d\n' "$cells"
    printf 'modes=%s\n' "$MODES"
    printf 'cases=%s\n' "$CASES"
    printf 'host=nproc=%s load=%s\n' "$(nproc 2>/dev/null || echo unknown)" "$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo unknown)"
    for mode in $MODES; do
        awk -F'\t' -v m="$mode" 'NR > 1 && $1 == m && $4 == 0 { print $6 }' "$OUT/results.tsv" > "$OUT/durations.$mode"
        percentiles "$mode" "$OUT/durations.$mode"
    done
    percentiles overall "$OUT/durations.all"
} > "$OUT/summary.txt"

cat "$OUT/summary.txt"
echo "smp-closeout: results=$OUT/results.tsv summary=$OUT/summary.txt"
exit 0
