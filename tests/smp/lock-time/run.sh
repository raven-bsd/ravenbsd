#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85a lock/time/IPI stress runner.
#
# Usage: sh tests/smp/lock-time/run.sh tcg|kvm|nomem|smp2|repro|variants|all
#
# The guest itself runs 50 iterations of 100 ms (5 s); the runner never retries a failed case.
# Every QEMU serial log is kept under build/smp/lock-time/logs/.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-tcg}
out=build/smp/lock-time/logs
mkdir -p "$out"
failed=0

run_one() {
    name=$1
    case=$2
    printf '== %s\n' "$name"
    if cargo xtask qtest --case "$case" --arch x86_64 > "$out/$name.qtest.log" 2>&1; then
        printf 'PASS %s\n' "$name"
    else
        printf 'FAIL %s\n' "$name"
        failed=1
    fi
    serial="build/qtest/$name.log"
    if [ -f "$serial" ]; then
        cp "$serial" "$out/$name.serial.log"
        iters=$(grep -c '^lock-time: iter=' "$serial" || true)
        printf '  iterations=%s\n' "$iters"
        grep '^lock-time: summary' "$serial" || true
    else
        printf '  serial log missing: %s\n' "$serial"
        failed=1
    fi
}

case "$mode" in
    tcg)
        run_one smp-lock-time-tcg tests/smp/lock-time/lock-time-tcg.toml
        ;;
    kvm)
        run_one smp-lock-time-kvm4 tests/smp/lock-time/lock-time-kvm4.toml
        run_one smp-lock-time-kvm2 tests/smp/lock-time/lock-time-kvm2.toml
        ;;
    nomem)
        run_one smp-lock-time-tcg-nomem tests/smp/lock-time/lock-time-tcg-nomem.toml
        ;;
    smp2)
        run_one smp-lock-time-tcg2 tests/smp/lock-time/lock-time-tcg2.toml
        ;;
    repro)
        run_one smp-lock-time-tlb-repro tests/smp/lock-time/lock-time-tlb-repro.toml
        ;;
    variants)
        run_one smp-lock-time-tcg-nomem tests/smp/lock-time/lock-time-tcg-nomem.toml
        run_one smp-lock-time-tcg2 tests/smp/lock-time/lock-time-tcg2.toml
        run_one smp-lock-time-tlb-repro tests/smp/lock-time/lock-time-tlb-repro.toml
        ;;
    all)
        run_one smp-lock-time-tcg tests/smp/lock-time/lock-time-tcg.toml
        run_one smp-lock-time-kvm4 tests/smp/lock-time/lock-time-kvm4.toml
        run_one smp-lock-time-kvm2 tests/smp/lock-time/lock-time-kvm2.toml
        run_one smp-lock-time-tcg-nomem tests/smp/lock-time/lock-time-tcg-nomem.toml
        run_one smp-lock-time-tcg2 tests/smp/lock-time/lock-time-tcg2.toml
        run_one smp-lock-time-tlb-repro tests/smp/lock-time/lock-time-tlb-repro.toml
        ;;
    *)
        printf 'usage: %s tcg|kvm|nomem|smp2|repro|variants|all\n' "$0" >&2
        exit 2
        ;;
esac

if [ "$failed" -ne 0 ]; then
    printf 'smp-lock-time: FAILED (logs in %s)\n' "$out" >&2
    exit 1
fi
printf 'smp-lock-time: PASSED\n'
