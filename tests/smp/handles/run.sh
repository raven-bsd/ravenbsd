#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K528 capability-lineage race on 1 or 4 CPUs.
#
# usage: sh tests/smp/handles/run.sh kvm1|kvm2|kvm4|all [--iterations N]
#
# Each iteration boots the same image once; a failure stops the campaign and is preserved.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-kvm4}
case "$mode" in
    kvm1|kvm2|kvm4|all) ;;
    *) printf 'usage: %s kvm1|kvm2|kvm4|all [--iterations N]\n' "$0" >&2; exit 2 ;;
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
    echo "smp-handles: BLOCKED /dev/kvm is not readable and writable" >&2
    exit 2
fi

out=build/smp/handles/logs
mkdir -p "$out"

run_case() {
    name=$1
    case_file=$2
    iteration=1
    while [ "$iteration" -le "$iterations" ]; do
        directory=$out/$name-$iteration
        mkdir -p "$directory"
        if ! timeout 180 cargo xtask qtest --case "$case_file" --arch x86_64 \
            > "$directory/qtest.log" 2>&1; then
            {
                echo "smp-handles: FAIL case=$name iteration=$iteration commit=$(git rev-parse HEAD)"
                tail -n 30 "$directory/qtest.log"
            } | tee "$out/failure.txt" >&2
            exit 1
        fi
        serial="build/qtest/$name.log"
        if [ -f "$serial" ]; then
            cp "$serial" "$directory/serial.log"
        else
            echo "smp-handles: missing serial log $serial" >&2
            exit 2
        fi
        iteration=$((iteration + 1))
    done
    printf 'PASS %s\n' "$name"
}

case "$mode" in
    kvm1)
        run_case smp-handles-kvm1 tests/smp/handles/handles-kvm1.toml
        ;;
    kvm2)
        run_case smp-handles-kvm2 tests/smp/handles/handles-kvm2.toml
        ;;
    kvm4)
        run_case smp-handles-kvm4 tests/smp/handles/handles-kvm4.toml
        ;;
    all)
        run_case smp-handles-kvm1 tests/smp/handles/handles-kvm1.toml
        run_case smp-handles-kvm2 tests/smp/handles/handles-kvm2.toml
        run_case smp-handles-kvm4 tests/smp/handles/handles-kvm4.toml
        ;;
esac

printf 'smp-handles: PASSED mode=%s iterations=%s\n' "$mode" "$iterations"
