#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K484 aarch64 memory-parity case and enforce the numeric s24 baseline row.
#
# usage: sh tests/smp/aarch64/run-memory.sh [--out DIR]

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

case_file=tests/smp/aarch64/smp-aarch64-memory.toml
case_name=smp-aarch64-memory
out=build/smp-aarch64/memory-run
if [ "${1:-}" = "--out" ] && [ "$#" -ge 2 ]; then
    out=$2
fi
rm -rf "$out"
mkdir -p "$out"

if ! cargo run -q -p raven-qtest -- run \
    --case "$case_file" \
    --arch aarch64 \
    --out "$out" \
    > "$out/stdout.txt" 2>&1; then
    {
        echo "aarch64-memory: FAIL case=$case_file"
        echo "aarch64-memory: log=$out/$case_name.log"
        echo "aarch64-memory: stdout=$out/stdout.txt"
        tail -n 30 "$out/stdout.txt"
    } >&2
    exit 1
fi

log=$out/$case_name.log
baseline=$(awk -F= '/^s24: frames free=/ { value=$2 } /^smp-aarch64-oom: round=1 children=2/ { print value; exit }' "$log")
post1=$(awk -F= '
    /^smp-aarch64-oom: round=1 children=2/ { armed=1; next }
    armed && /^s24: frames free=/ { value=$2 }
    /^smp-aarch64-oom: round=1 done/ { if (value != "") print value; exit }
' "$log")
post2=$(awk -F= '
    /^smp-aarch64-oom: round=2 children=2/ { armed=1; next }
    armed && /^s24: frames free=/ { value=$2 }
    /^smp-aarch64-oom: round=2 done/ { if (value != "") print value; exit }
' "$log")
for value in "$baseline" "$post1" "$post2"; do
    case $value in
        ''|*[!0-9]*)
            {
                echo "aarch64-memory: FAIL baseline=$baseline post1=$post1 post2=$post2 (missing s24 or oom marker)"
                echo "aarch64-memory: log=$log"
            } >&2
            exit 1
            ;;
    esac
done

# The test-owned shared page and child stacks plus the K453 one-time reserve and the documented
# TCG cache transient. A reclaim leak is far larger and fails this row.
allowed=$((4096 + 1052672 + 32768))
if [ "$post1" -lt $((baseline - allowed)) ]; then
    {
        echo "aarch64-memory: FAIL oom-baseline baseline=$baseline post1=$post1 delta=$((baseline - post1)) allowed=$allowed"
        echo "aarch64-memory: log=$log"
    } >&2
    exit 1
fi
if [ "$post2" -lt $((post1 - 32768)) ]; then
    {
        echo "aarch64-memory: FAIL oom-per-attempt post1=$post1 post2=$post2 delta=$((post1 - post2)) allowed=32768"
        echo "aarch64-memory: log=$log"
    } >&2
    exit 1
fi

echo "aarch64-memory: PASS baseline=$baseline post1=$post1 post2=$post2 delta=$((baseline - post1)) allowed=$allowed"
echo "aarch64-memory: log=$log"
