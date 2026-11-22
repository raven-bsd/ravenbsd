#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K496 duplex semantics matrix.
#
# usage: sh tests/smp/duplex-matrix/run.sh [tcg|kvm2] [--iterations N] [--no-build]
#
# One iteration is a determinism pair: the same image is booted twice and the two trace hashes must
# match. The first failure stops the campaign and is preserved; no retry hides it.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-tcg}
case "$mode" in
    tcg|kvm2) ;;
    *) printf 'usage: %s [tcg|kvm2] [--iterations N] [--no-build]\n' "$0" >&2; exit 2 ;;
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
    tcg)
        case_file=tests/smp/duplex-matrix/duplex-matrix.toml
        case_name=duplex-matrix
        smp=4
        kvm_state=disabled
        boot_cap=75
        ;;
    kvm2)
        case_file=tests/smp/duplex-matrix/duplex-matrix-kvm2.toml
        case_name=duplex-matrix-kvm2
        smp=2
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "duplex-matrix: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        kvm_state="enabled $(ls -l /dev/kvm)"
        boot_cap=75
        ;;
esac

image=build/duplex-matrix/duplex-matrix.img
initfs=build/duplex-matrix/initfs.img
cmdline='console=ttyS0 test-exit=1 smp_user=on'
out=build/duplex-matrix/$mode

if [ "$no_build" -eq 0 ]; then
    sh tests/smp/duplex-matrix/build.sh
    cargo xtask image --arch x86_64 --profile base --cmdline "$cmdline" \
        --initfs "$initfs" --output "$image"
fi
for required in "$image" "$initfs" "$case_file"; do
    if [ ! -f "$required" ]; then
        echo "duplex-matrix: missing $required (run without --no-build first)" >&2
        exit 2
    fi
done

boot() {
    directory=$1
    mkdir -p "$directory"
    if ! RAVEN_DUPLEX_MATRIX_NO_BUILD=1 timeout "$boot_cap" \
        cargo run -q -p raven-qtest -- run \
            --case "$case_file" \
            --arch x86_64 \
            --image "$image" \
            --no-build \
            --out "$directory" \
            > "$directory/stdout.txt" 2>&1; then
        {
            echo "duplex-matrix: FAIL mode=$mode directory=$directory commit=$(git rev-parse HEAD)"
            echo "duplex-matrix: case=$case_file image=$image"
            echo "duplex-matrix: log=$directory/$case_name.log"
            echo "duplex-matrix: stdout=$directory/stdout.txt"
            tail -n 30 "$directory/stdout.txt"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    hash=$(sed -n 's/.*table-hash=\(0x[0-9a-f][0-9a-f]*\).*/\1/p' "$directory/$case_name.log" | tail -n 1)
    if [ -z "$hash" ]; then
        echo "duplex-matrix: missing table hash in $directory/$case_name.log" >&2
        exit 2
    fi
    printf '%s\n' "$hash"
}

rm -rf "$out"
mkdir -p "$out"
iteration=1
while [ "$iteration" -le "$iterations" ]; do
    run_dir=$out/run-$iteration
    hash_a=$(boot "$run_dir/a")
    hash_b=$(boot "$run_dir/b")
    if [ "$hash_a" != "$hash_b" ]; then
        echo "duplex-matrix: determinism FAIL iteration=$iteration a=$hash_a b=$hash_b" >&2
        exit 1
    fi
    cp "$run_dir/a/$case_name.log" "$out/run-$iteration.log"
    printf '%s\n' "$hash_a" > "$run_dir/hash.txt"
    iteration=$((iteration + 1))
done

last_log=$out/run-$iterations/a/$case_name.log
rows=$(sed -n 's/.*summary rows=\([0-9][0-9]*\).*/\1/p' "$last_log" | tail -n 1)
deviations=$(sed -n 's/.*deviations=\([0-9][0-9]*\).*/\1/p' "$last_log" | tail -n 1)
trace=$(sed -n 's/.*trace-hash=\(0x[0-9a-f][0-9a-f]*\).*/\1/p' "$out/run-$iterations.log" | tail -n 1)

{
    echo "K496 duplex semantics matrix"
    echo "mode=$mode smp=$smp kvm=$kvm_state"
    echo "commit=$(git rev-parse HEAD)"
    echo "qemu=$(qemu-system-x86_64 --version | head -n 1)"
    echo "case=$case_file image=$image seed=0x4b34393644555000"
    echo "command=sh tests/smp/duplex-matrix/run.sh $mode --iterations $iterations"
    echo "iterations=$iterations boots=$((iterations * 2)) determinism=identical-hash"
    echo "table-hash=$(cat "$out"/run-*/hash.txt | tail -n 1)"
    echo "fuzz-trace-hash=$trace"
    echo "rows=$rows deviations=$deviations"
} | tee "$out/summary.txt"
