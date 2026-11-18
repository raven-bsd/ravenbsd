#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85f: aarch64 TCG -smp 4 boot/stress matrix plus cross-arch libc/threads.
# S106 adds the four-way concurrent unmap/TLB-shootdown regression to the fast loop.
#
# Runs the existing aarch64 SMP cases on QEMU TCG with -smp 4, a documented soak/time-box, the
# x86_64 counterparts and `libc-threads` on both arches. A failing iteration stops the matrix
# (no retry that hides a failure). KVM aarch64 is probed but never faked: the binary must offer
# the accelerator and the probe must boot.
#
# Usage: scripts/smp-aarch64.sh [--iterations N] [--timebox SECS] [--soak N] [--max-load N] [--kvm]
# Outputs: build/qtest/smp-aarch64.tsv (case/arch/iteration/rc/duration_ms)
#          build/qtest/smp-aarch64/*.log (one serial transcript per run)

set -u

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

ITERATIONS=50
TIMEBOX=5400
SOAK=5
XARCH=5
KVM=0
MAX_LOAD=15

while [ $# -gt 0 ]; do
    case "$1" in
        --iterations) ITERATIONS=$2; shift 2 ;;
        --timebox) TIMEBOX=$2; shift 2 ;;
        --soak) SOAK=$2; shift 2 ;;
        --max-load) MAX_LOAD=$2; shift 2 ;;
        --kvm) KVM=1; shift ;;
        -h|--help)
            echo "usage: $0 [--iterations N] [--timebox SECS] [--soak N] [--max-load N] [--kvm]"
            exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

# S103 rework: refuse to start under a busy host or a competing QEMU soak. The matrix is a
# reliability gate; overlapping another QEMU run is not a test failure and must not be recorded
# as one. `--max-load 0` disables the guard for a deliberate manual run.
if [ "$MAX_LOAD" -gt 0 ]; then
    load1=$(cut -d ' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
    if awk -v load1v="$load1" -v maxv="$MAX_LOAD" 'BEGIN { exit !(load1v > maxv) }'; then
        echo "smp-aarch64: REFUSED load1=$load1 > max_load=$MAX_LOAD (busy host/QEMU overlap)" >&2
        exit 3
    fi
fi

OUT=build/qtest
LOG="$OUT/smp-aarch64.tsv"
LOGDIR="$OUT/smp-aarch64"
mkdir -p "$LOGDIR"
: > "$LOG"

START=$(date +%s)
TIMED_OUT=0
FAILED=0
ITER=0

elapsed() { echo $(( $(date +%s) - START )); }

# run_case <case> <arch> <iteration>
run_case() {
    case_name=$1
    arch=$2
    iter=$3
    if [ "$(elapsed)" -ge "$TIMEBOX" ]; then
        TIMED_OUT=1
        return 0
    fi
    t0=$(date +%s)
    log="$LOGDIR/${case_name}-${arch}-${iter}.log"
    if cargo xtask qtest --case "$case_name" --arch "$arch" > "$log" 2>&1; then
        rc=0
    else
        rc=$?
    fi
    duration=$(sed -n 's/.*"duration_ms": \([0-9][0-9]*\).*/\1/p' "$OUT/$case_name.json" 2>/dev/null | head -1)
    [ -n "$duration" ] || duration=$(( ($(date +%s) - t0) * 1000 ))
    printf '%s\t%s\t%s\t%s\t%s\n' "$case_name" "$arch" "$iter" "$rc" "$duration" >> "$LOG"
    if [ "$rc" -ne 0 ]; then
        FAILED=1
        echo "FAIL $case_name $arch iter=$iter rc=$rc log=$log" >&2
        return 1
    fi
    return 0
}

echo "smp-aarch64: commit=$(git rev-parse --short HEAD) iterations=$ITERATIONS timebox=${TIMEBOX}s soak=$SOAK max_load=$MAX_LOAD kvm=$KVM"
KVM_STATUS="unavailable"
if [ "$KVM" -eq 1 ]; then
    if qemu-system-aarch64 -accel help 2>&1 | grep -qx 'kvm' \
        && timeout 10 qemu-system-aarch64 -machine virt -accel kvm -cpu host -m 512 -smp 2 \
            -nographic -serial none -monitor none -no-reboot > "$LOGDIR/kvm-probe.log" 2>&1; then
        KVM_STATUS="available"
    else
        echo "KVM aarch64: UNAVAILABLE on this host (see $LOGDIR/kvm-probe.log)" >&2
    fi
fi
if qemu-system-aarch64 -accel help 2>&1 | grep -qx 'kvm'; then
    KVM_OFFERED=1
else
    KVM_OFFERED=0
fi
printf 'KVM aarch64 offered=%s status=%s\n' "$KVM_OFFERED" "$KVM_STATUS" > "$OUT/smp-aarch64.kvm.txt"
KVM_BLOCKED=0
if [ "$KVM" -eq 1 ] && [ "$KVM_STATUS" != "available" ]; then
    KVM_BLOCKED=1
fi

# TCG aarch64: 50 fast iterations of the user, single-controller TLB and four-way concurrent
# unmap/TLB-shootdown proofs (time-boxed). S106 adds the four-way waiter regression.
while [ "$ITER" -lt "$ITERATIONS" ]; do
    run_case smp-user-aarch64 aarch64 "$ITER" || break
    run_case smp-tlb-aarch64 aarch64 "$ITER" || break
    run_case smp-tlb-aarch64-concurrent aarch64 "$ITER" || break
    ITER=$((ITER + 1))
done

# A failure in the fast loop stops the whole matrix: the soak and cross-arch rows are not
# meaningful evidence once the aarch64 gate has failed, and running them under load would only
# create more flaky rows. Preserve the fast-loop logs and report the failure.
soak_iter=0
x_iter=0
if [ "$FAILED" -eq 0 ]; then
    # Documented soak time-box: the 60 s soak is not 50x feasible; run SOAK iterations.
    while [ "$soak_iter" -lt "$SOAK" ]; do
        run_case smp-soak-aarch64 aarch64 "$soak_iter" || break
        soak_iter=$((soak_iter + 1))
    done
fi

if [ "$FAILED" -eq 0 ]; then
    # Cross-arch x86_64 counterparts and libc/threads comparison.
    while [ "$x_iter" -lt "$XARCH" ]; do
        run_case smp-user x86_64 "$x_iter" || break
        run_case smp-soak x86_64 "$x_iter" || break
        run_case libc-threads x86_64 "$x_iter" || break
        run_case libc-threads aarch64 "$x_iter" || break
        x_iter=$((x_iter + 1))
    done
fi

# Summary: count, fail, min, p50, p90, max per case/arch. The log is sorted by
# case/arch/duration first, so each key's values arrive sorted.
{
    echo "case	arch	runs	fail	min_ms	p50_ms	p90_ms	max_ms"
    awk -F '	' '{ print $1 "	" $2 "	" $5 "	" $4 }' "$LOG" \
        | sort -t '	' -k1,1 -k2,2 -k3,3n \
        | awk -F '	' '
            function flush() {
                if (key == "") return
                p50 = vals[int((n + 1) / 2)]
                idx = int((9 * n + 9) / 10); if (idx > n) idx = n
                p90 = vals[idx]
                printf "%s\t%d\t%d\t%d\t%d\t%d\t%d\n", key, n, fail + 0, vals[1], p50, p90, vals[n]
            }
            {
                newkey = $1 " " $2
                if (newkey != key) { flush(); key = newkey; n = 0; fail = 0 }
                n++; vals[n] = $3
                if ($4 != 0) fail++
            }
            END { flush() }
        '
} > "$OUT/smp-aarch64.summary.tsv"

cat "$OUT/smp-aarch64.summary.tsv"
echo "smp-aarch64: fast_iterations=$ITER soak_iterations=$soak_iter cross_arch_iterations=$x_iter timed_out=$TIMED_OUT failed=$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
if [ "$KVM_BLOCKED" -eq 1 ]; then
    echo "smp-aarch64: KVM aarch64 BLOCKED (accelerator unavailable or probe failed)" >&2
    exit 77
fi
[ "$TIMED_OUT" -eq 0 ] || echo "smp-aarch64: time-box ${TIMEBOX}s reached; matrix recorded" >&2
exit 0
