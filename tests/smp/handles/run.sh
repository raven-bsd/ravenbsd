#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K528 capability-lineage campaign.
#
# usage: sh tests/smp/handles/run.sh [kvm1|kvm4] [--iterations N] [--no-build]
#
# The fixture is built once and each iteration boots the image with a fresh QEMU. The first
# failure stops the campaign and is preserved; no retry hides it.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-kvm4}
case "$mode" in
    kvm1|kvm4) ;;
    *) printf 'usage: %s [kvm1|kvm4] [--iterations N] [--no-build]\n' "$0" >&2; exit 2 ;;
esac
shift || true

iterations=1
no_build=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --iterations)
            [ "$#" -ge 2 ] || { echo "--iterations needs a value" >&2; exit 2; }
            iterations=$2
            shift 2
            ;;
        --no-build)
            no_build=1
            shift
            ;;
        *)
            echo "unknown option: $1" >&2
            exit 2
            ;;
    esac
done

case "$mode" in
    kvm1)
        case_file=tests/smp/handles/handles-kvm1.toml
        case_name=handles-kvm1
        smp=1
        kvm_state="enabled $(ls -l /dev/kvm)"
        ;;
    kvm4)
        case_file=tests/smp/handles/handles-kvm4.toml
        case_name=handles-kvm4
        smp=4
        kvm_state="enabled $(ls -l /dev/kvm)"
        ;;
esac

if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo "handles: BLOCKED /dev/kvm is not readable and writable" >&2
    exit 2
fi

image=build/handles/handles.img
initfs=build/handles/initfs.img
cmdline='console=ttyS0 test-exit=1 smp_user=on'
out=build/handles/$mode

if [ "$no_build" -eq 0 ]; then
    sh tests/smp/handles/build.sh
    cargo xtask image --arch x86_64 --profile base --cmdline "$cmdline" \
        --initfs "$initfs" --output "$image"
fi
for required in "$image" "$initfs" "$case_file"; do
    if [ ! -f "$required" ]; then
        echo "handles: missing $required (run without --no-build first)" >&2
        exit 2
    fi
done

rm -rf "$out"
mkdir -p "$out"
iteration=1
while [ "$iteration" -le "$iterations" ]; do
    directory=$out/run-$iteration
    mkdir -p "$directory"
    if ! RAVEN_HANDLES_NO_BUILD=1 timeout 240 \
        cargo run -q -p raven-qtest -- run \
            --case "$case_file" \
            --arch x86_64 \
            --image "$image" \
            --no-build \
            --no-pre \
            --out "$directory" \
            > "$directory/stdout.txt" 2>&1; then
        {
            echo "handles: FAIL mode=$mode iteration=$iteration commit=$(git rev-parse HEAD)"
            echo "handles: case=$case_file image=$image"
            echo "handles: log=$directory/$case_name.log"
            echo "handles: stdout=$directory/stdout.txt"
            tail -n 30 "$directory/stdout.txt"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    cp "$directory/$case_name.log" "$out/run-$iteration.log"
    iteration=$((iteration + 1))
done

last_log=$out/run-$iterations.log
rows=$(sed -n 's/.*summary rows=\([0-9][0-9]*\).*/\1/p' "$last_log" | tail -n 1)
{
    echo "K528 capability lineage"
    echo "mode=$mode smp=$smp kvm=$kvm_state"
    echo "commit=$(git rev-parse HEAD)"
    echo "qemu=$(qemu-system-x86_64 --version | head -n 1)"
    echo "case=$case_file image=$image"
    echo "command=sh tests/smp/handles/run.sh $mode --iterations $iterations"
    echo "iterations=$iterations boots=$iterations"
    echo "rows=$rows"
} | tee "$out/summary.txt"
