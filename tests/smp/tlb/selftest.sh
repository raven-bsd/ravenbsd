#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Fast self-test for the K376 load generator and the tlb-stress K376 CLI.
# It runs no QEMU and no load-matched sample; the real gate evidence lives in the report.

set -eu
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$ROOT" || exit 1

tmp=$(mktemp -d "${TMPDIR:-/tmp/opencode}/k376-selftest.XXXXXX") || exit 1
trap 'sh tests/smp/tlb/load-generator.sh stop "$tmp/load.pids" >/dev/null 2>&1 || true; rm -rf "$tmp"' EXIT INT TERM

fail() {
    echo "selftest: FAIL: $*" >&2
    exit 1
}

# The load generator must start exactly N workers and stop exactly those workers.
sh tests/smp/tlb/load-generator.sh start 3 "$tmp/load.pids" > "$tmp/start.out"
started=$(wc -l < "$tmp/load.pids" | tr -d ' ')
[ "$started" -eq 3 ] || fail "start worker count $started != 3"
cp "$tmp/load.pids" "$tmp/pids"
while IFS= read -r pid; do
    [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null || fail "worker $pid is not running"
done < "$tmp/pids"
sh tests/smp/tlb/load-generator.sh stop "$tmp/load.pids" > "$tmp/stop.out"
while IFS= read -r pid; do
    [ -n "$pid" ] || continue
    if kill -0 "$pid" 2>/dev/null; then
        fail "worker $pid survived stop"
    fi
done < "$tmp/pids"
[ ! -f "$tmp/load.pids" ] || fail "stop left the pidfile"

# K376-only options require --load and never fall through to the S120 matrix.
if sh scripts/tlb-stress.sh --iterations 3 > "$tmp/extra.out" 2>&1; then
    fail "--iterations without --load was accepted"
fi
grep -q 'require --load' "$tmp/extra.out" || fail "missing --load requirement message"

# Invalid load and iteration values are refused before any sample.
if sh scripts/tlb-stress.sh --load x > "$tmp/load.out" 2>&1; then
    fail "--load x was accepted"
fi
grep -q 'non-negative integer' "$tmp/load.out" || fail "missing invalid --load message"
if sh scripts/tlb-stress.sh --load 0 --iterations 0 > "$tmp/iter.out" 2>&1; then
    fail "--iterations 0 was accepted"
fi
grep -q 'at least 1' "$tmp/iter.out" || fail "missing invalid --iterations message"

# K376 runs one case; a comma list is refused before any sample.
if sh scripts/tlb-stress.sh --load 0 --case a,b > "$tmp/case.out" 2>&1; then
    fail "a case list was accepted"
fi
grep -q 'must not be a list' "$tmp/case.out" || fail "missing case-list message"

# The S120 matrix mode must remain available.
sh scripts/tlb-stress.sh --help > "$tmp/help.out"
grep -q -- '--runs N' "$tmp/help.out" || fail "S120 --runs is missing from help"
grep -q -- '--load N' "$tmp/help.out" || fail "K376 --load is missing from help"

echo "selftest: PASS"
