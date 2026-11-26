#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K530 kernel stack lifetime fixture on 1, 2 or 4 CPUs.
#
# usage: sh tests/smp/stack/run.sh kvm1|kvm2|kvm4 [--iterations N]
#
# The guest creates 80 threads, releases them, and repeats for 10 cycles (800 admissions, below
# the 1024 live-thread bound). The kernel `s69` line at exit carries `stack_fresh`,
# `stack_cached` and `heap`; this script asserts that the fresh stack count stays near the first
# cycle, every freed stack is cached, no pair was dropped, and the shipped fixture's reuse probe
# printed `reuse-zero ok`.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-kvm4}
case "$mode" in
    kvm1|kvm2|kvm4) ;;
    *) printf 'usage: %s kvm1|kvm2|kvm4 [--iterations N]\n' "$0" >&2; exit 2 ;;
esac
shift || true

iterations=1
while [ "$#" -gt 0 ]; do
    case "$1" in
        --iterations)
            [ "$#" -ge 2 ] || { echo "--iterations needs a value" >&2; exit 2; }
            iterations=$2
            shift 2
            ;;
        *)
            echo "unknown option: $1" >&2
            exit 2
            ;;
    esac
done

if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo "smp-stack: BLOCKED /dev/kvm is not readable and writable" >&2
    exit 2
fi

out=build/smp/stack/logs
mkdir -p "$out"

iteration=1
while [ "$iteration" -le "$iterations" ]; do
    directory=$out/run-$iteration-$mode
    mkdir -p "$directory"
    if ! timeout 400 cargo xtask qtest --case "tests/smp/stack/stack-$mode.toml" --arch x86_64 \
        > "$directory/qtest.log" 2>&1; then
        {
            echo "smp-stack: FAIL iteration=$iteration mode=$mode commit=$(git rev-parse HEAD)"
            tail -n 40 "$directory/qtest.log"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    serial="build/qtest/smp-stack-$mode.log"
    cp "$serial" "$directory/serial.log" 2>/dev/null || true
    line=$(grep -E '^s69: exit s69-stack ' "$serial" | tail -n 1 || true)
    if [ -z "$line" ]; then
        echo "smp-stack: FAIL no s69 stack line in $serial" >&2
        exit 1
    fi
    fresh=$(printf '%s\n' "$line" | sed -n 's/.*stack_fresh=\([0-9][0-9]*\).*/\1/p')
    cached=$(printf '%s\n' "$line" | sed -n 's/.*stack_cached=\([0-9][0-9]*\).*/\1/p')
    heap=$(printf '%s\n' "$line" | sed -n 's/.*heap=\([0-9][0-9]*\).*/\1/p')
    if [ -z "$fresh" ] || [ -z "$cached" ] || [ -z "$heap" ]; then
        echo "smp-stack: FAIL unparsable s69 line: $line" >&2
        exit 1
    fi
    if ! grep -q '^stack-selftest: reuse-zero ok$' "$serial"; then
        echo "smp-stack: FAIL reuse-zero probe missing or stale" >&2
        grep -E '^stack-selftest:|^stack: fail' "$serial" | tail -n 10 >&2 || true
        exit 1
    fi
    if ! grep -q '^stack-selftest: guard-unmapped ok$' "$serial"; then
        echo "smp-stack: FAIL guard probe missing or mapped" >&2
        grep -E '^stack-selftest:|^stack: fail' "$serial" | tail -n 10 >&2 || true
        exit 1
    fi
    # Green fresh is 5 init/kernel + 1 selftest + 80 churn = 81; the 64-entry mutation carves 16
    # new stacks every cycle (81 + 9 * 16 = 225), so 100 is a clean discriminator that is also
    # below the 1024 admission bound. The cache must hold all 80 churn stacks, and the one-shot
    # `stack-cache: DROPPED` line is the direct leak evidence.
    if [ "$fresh" -gt 100 ] || [ "$cached" -lt 80 ]; then
        echo "smp-stack: FAIL mode=$mode stack_fresh=$fresh stack_cached=$cached" >&2
        exit 1
    fi
    if grep -q '^stack-cache: DROPPED ' "$serial"; then
        echo "smp-stack: FAIL mode=$mode a freed stack pair was dropped" >&2
        exit 1
    fi
    printf 'smp-stack: PASSED mode=%s iteration=%s stack_fresh=%s stack_cached=%s heap=%s\n' \
        "$mode" "$iteration" "$fresh" "$cached" "$heap"
    iteration=$((iteration + 1))
done
