#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K543 readiness contract on 4 CPUs.
#
# usage: sh tests/smp/readiness/run.sh kvm4 [--iterations N]

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-kvm4}
case "$mode" in
    kvm4) ;;
    *) printf 'usage: %s kvm4 [--iterations N]\n' "$0" >&2; exit 2 ;;
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
    echo "smp-readiness: BLOCKED /dev/kvm is not readable and writable" >&2
    exit 2
fi

out=build/smp/readiness/logs
mkdir -p "$out"

iteration=1
while [ "$iteration" -le "$iterations" ]; do
    directory=$out/run-$iteration
    mkdir -p "$directory"
    if ! timeout 180 cargo xtask qtest --case tests/smp/readiness/readiness-kvm4.toml --arch x86_64 \
        > "$directory/qtest.log" 2>&1; then
        {
            echo "smp-readiness: FAIL iteration=$iteration commit=$(git rev-parse HEAD)"
            tail -n 30 "$directory/qtest.log"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    cp build/qtest/smp-readiness-kvm4.log "$directory/serial.log" 2>/dev/null || true
    iteration=$((iteration + 1))
done

printf 'smp-readiness: PASSED mode=%s iterations=%s\n' "$mode" "$iterations"
