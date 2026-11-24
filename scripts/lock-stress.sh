#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Bounded SMP stress harness for raven-turnstile and raven-cyclic.
# The tests are pure model tests; this script supplies parallel workers, host
# load, per-iteration logs and latency/jitter samples.

set -u

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

LOAD=64
ITERATIONS=10
WORKERS=4
TEST_TIMEOUT=120
OUT=""

usage() {
    cat >&2 <<'EOF'
usage: scripts/lock-stress.sh [--load N] [--iterations M] [--workers W] [--out DIR]
  --load N       target 1-minute host load; burners are added only below N
  --iterations M measured iterations (default 10)
  --workers W    parallel test workers per binary (default 4)
  --out DIR      evidence directory (default /tmp/opencode/lock-stress-<UTC>)
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --load) [ "$#" -ge 2 ] || { usage; exit 2; }; LOAD=$2; shift 2 ;;
        --iterations) [ "$#" -ge 2 ] || { usage; exit 2; }; ITERATIONS=$2; shift 2 ;;
        --workers) [ "$#" -ge 2 ] || { usage; exit 2; }; WORKERS=$2; shift 2 ;;
        --out) [ "$#" -ge 2 ] || { usage; exit 2; }; OUT=$2; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
    esac
done

case "$LOAD" in ''|*[!0-9]*) echo "--load must be a non-negative integer" >&2; exit 2 ;; esac
case "$ITERATIONS" in ''|*[!0-9]*) echo "--iterations must be a positive integer" >&2; exit 2 ;; esac
case "$WORKERS" in ''|*[!0-9]*) echo "--workers must be a positive integer" >&2; exit 2 ;; esac
[ "$ITERATIONS" -ge 1 ] || { echo "--iterations must be >= 1" >&2; exit 2; }
[ "$WORKERS" -ge 1 ] || { echo "--workers must be >= 1" >&2; exit 2; }

if [ -z "$OUT" ]; then
    OUT="/tmp/opencode/lock-stress-$(date -u +%Y%m%dT%H%M%SZ)"
fi
mkdir -p "$OUT/iter" "$OUT/samples" || exit 1
META="$OUT/binaries.json"
SUMMARY="$OUT/iterations.tsv"
printf 'iteration\tworker\tbinary\texit\twall_ns\tload1\tload5\tload15\tlog\n' > "$SUMMARY"

echo "lock-stress: building test binaries with nextest" >&2
if ! cargo nextest list -p raven-turnstile -p raven-cyclic \
    --list-type binaries-only --message-format json > "$META" 2> "$OUT/nextest-list.log"; then
    echo "lock-stress: FAIL nextest list; log $OUT/nextest-list.log" >&2
    exit 1
fi
BINS=$(grep -o '"binary-path":"[^"]*"' "$META" | sed 's/.*:"//;s/"$//')
if [ -z "$BINS" ]; then
    echo "lock-stress: FAIL no test binaries in $META" >&2
    exit 1
fi

read -r load1 load5 load15 _ < /proc/loadavg
current=${load1%.*}
burn=$((LOAD - current))
[ "$burn" -lt 0 ] && burn=0
load_pids=""
cleanup() {
    for pid in $load_pids; do
        kill "$pid" 2>/dev/null || true
    done
}
trap cleanup EXIT INT TERM

i=0
while [ "$i" -lt "$burn" ]; do
    sh -c 'while :; do :; done' &
    load_pids="$load_pids $!"
    i=$((i + 1))
done

run_worker() {
    iter=$1
    worker=$2
    bin=$3
    name=$(basename "$bin")
    log="$OUT/iter/$iter-worker-$worker-$name.log"
    sample="$OUT/samples/$iter-worker-$worker-$name.tsv"
    start=$(date +%s%N)
    LOCK_STRESS_SAMPLES="$sample" timeout "$TEST_TIMEOUT" "$bin" stress_ > "$log" 2>&1
    rc=$?
    end=$(date +%s%N)
    wall=$((end - start))
    read -r l1 l5 l15 _ < /proc/loadavg
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$iter" "$worker" "$name" "$rc" "$wall" "$l1" "$l5" "$l15" "$log" >> "$SUMMARY"
    if [ "$rc" -ne 0 ]; then
        echo "lock-stress: FAIL iteration=$iter worker=$worker binary=$name exit=$rc log=$log" >&2
        return 1
    fi
    return 0
}

echo "lock-stress: iterations=$ITERATIONS workers=$WORKERS load_target=$LOAD burners=$burn" >&2
iter=1
while [ "$iter" -le "$ITERATIONS" ]; do
    jobs=""
    worker=1
    for bin in $BINS; do
        copy=1
        while [ "$copy" -le "$WORKERS" ]; do
            run_worker "$iter" "$worker-$copy" "$bin" &
            jobs="$jobs $!"
            copy=$((copy + 1))
        done
        worker=$((worker + 1))
    done
    failed=0
    for pid in $jobs; do
        wait "$pid" || failed=1
    done
    if [ "$failed" -ne 0 ]; then
        echo "lock-stress: BLOCKED on a red iteration; evidence kept at $OUT" >&2
        exit 1
    fi
    iter=$((iter + 1))
done

cleanup
trap - EXIT INT TERM

cat "$OUT"/samples/*.tsv > "$OUT/samples/all.tsv" 2>/dev/null || true
if [ -s "$OUT/samples/all.tsv" ]; then
    sort -k1,1 -k2,2n "$OUT/samples/all.tsv" | awk -F'\t' '
        function idx(p, c, i) {
            i = int((p * c + 99) / 100);
            if (i < 1) i = 1;
            if (i > c) i = c;
            return i;
        }
        {
            label = $1;
            n[label] += 1;
            value[label, n[label]] = $2 + 0;
        }
        END {
            for (label in n) {
                c = n[label];
                printf "%s samples=%d p50_ns=%d p90_ns=%d p99_ns=%d max_ns=%d\n",
                    label, c,
                    value[label, idx(50, c)],
                    value[label, idx(90, c)],
                    value[label, idx(99, c)],
                    value[label, c];
            }
        }' | sort
else
    echo "lock-stress: FAIL no samples collected" >&2
    exit 1
fi

echo "lock-stress: PASS iterations=$ITERATIONS workers=$WORKERS load_target=$LOAD burners=$burn"
echo "lock-stress: evidence=$OUT"
