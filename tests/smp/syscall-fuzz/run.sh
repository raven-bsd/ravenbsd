#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the K456 syscall robustness measurement.
#
# usage: sh tests/smp/syscall-fuzz/run.sh [tcg|kvm2] [--iterations N] [--arch x86_64|aarch64] [--no-build]
#
# One iteration is a determinism pair: the same image is booted twice with the same seed and the
# two table hashes must match. The first failure stops the campaign and is preserved; no retry
# hides it. `iterations=1` therefore means two boots.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${1:-tcg}
case "$mode" in
    tcg|kvm2) ;;
    *) printf 'usage: %s [tcg|kvm2] [--iterations N] [--arch x86_64|aarch64] [--no-build]\n' "$0" >&2; exit 2 ;;
esac
shift || true

iterations=1
no_build=0
arch=x86_64
while [ "$#" -gt 0 ]; do
    case "$1" in
        --iterations)
            [ "$#" -ge 2 ] || { echo "--iterations needs a value" >&2; exit 2; }
            iterations=$2
            shift 2
            ;;
        --arch)
            [ "$#" -ge 2 ] || { echo "--arch needs a value" >&2; exit 2; }
            arch=$2
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

case "$arch" in
    x86_64|aarch64) ;;
    *) echo "syscall-fuzz: unsupported arch $arch (x86_64 or aarch64)" >&2; exit 2 ;;
esac

case "$mode" in
    tcg)
        smp=4
        kvm_state=disabled
        if [ "$arch" = aarch64 ]; then
            case_file=tests/smp/syscall-fuzz/syscall-fuzz-aarch64.toml
            case_name=syscall-fuzz-aarch64
            image=build/syscall-fuzz/aarch64/syscall-fuzz.img
            initfs=build/syscall-fuzz/aarch64/initfs.img
            cmdline='console=ttyAMA0 test-exit=1 smp_user=on'
            out=build/syscall-fuzz/aarch64/$mode
        else
            case_file=tests/smp/syscall-fuzz/syscall-fuzz.toml
            case_name=syscall-fuzz
            image=build/syscall-fuzz/syscall-fuzz.img
            initfs=build/syscall-fuzz/initfs.img
            cmdline='console=ttyS0 test-exit=1 smp_user=on'
            out=build/syscall-fuzz/$mode
        fi
        ;;
    kvm2)
        [ "$arch" = x86_64 ] || {
            echo "syscall-fuzz: kvm2 is x86_64 only (use tcg --arch aarch64)" >&2
            exit 2
        }
        case_file=tests/smp/syscall-fuzz/syscall-fuzz-kvm2.toml
        case_name=syscall-fuzz-kvm2
        smp=2
        if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
            echo "syscall-fuzz: BLOCKED /dev/kvm is not readable and writable" >&2
            exit 2
        fi
        kvm_state="enabled $(ls -l /dev/kvm)"
        image=build/syscall-fuzz/syscall-fuzz.img
        initfs=build/syscall-fuzz/initfs.img
        cmdline='console=ttyS0 test-exit=1 smp_user=on'
        out=build/syscall-fuzz/$mode
        ;;
esac

if [ "$no_build" -eq 0 ]; then
    sh tests/smp/syscall-fuzz/raw-check.sh
    sh tests/smp/syscall-fuzz/build.sh "$arch"
    cargo xtask image --arch "$arch" --profile base --cmdline "$cmdline" \
        --initfs "$initfs" --output "$image"
fi
for required in "$image" "$initfs" "$case_file"; do
    if [ ! -f "$required" ]; then
        echo "syscall-fuzz: missing $required (run without --no-build first)" >&2
        exit 2
    fi
done

boot() {
    directory=$1
    mkdir -p "$directory"
    if ! RAVEN_SYSCALL_FUZZ_NO_BUILD=1 timeout 150 \
        cargo run -q -p raven-qtest -- run \
            --case "$case_file" \
            --arch "$arch" \
            --image "$image" \
            --no-build \
            --out "$directory" \
            > "$directory/stdout.txt" 2>&1; then
        {
            echo "syscall-fuzz: FAIL mode=$mode directory=$directory commit=$(git rev-parse HEAD)"
            echo "syscall-fuzz: case=$case_file image=$image"
            echo "syscall-fuzz: log=$directory/$case_name.log"
            echo "syscall-fuzz: stdout=$directory/stdout.txt"
            tail -n 30 "$directory/stdout.txt"
        } | tee "$out/failure.txt" >&2
        exit 1
    fi
    hash=$(sed -n 's/.*table-hash=\(0x[0-9a-f][0-9a-f]*\).*/\1/p' "$directory/$case_name.log" | tail -n 1)
    if [ -z "$hash" ]; then
        echo "syscall-fuzz: missing table hash in $directory/$case_name.log" >&2
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
        echo "syscall-fuzz: determinism FAIL iteration=$iteration a=$hash_a b=$hash_b" >&2
        exit 1
    fi
    cp "$run_dir/a/$case_name.log" "$out/run-$iteration.log"
    printf '%s\n' "$hash_a" > "$run_dir/hash.txt"
    iteration=$((iteration + 1))
done

last_log=$out/run-$iterations/a/$case_name.log
rows=$(sed -n 's/.*summary rows=\([0-9][0-9]*\).*/\1/p' "$last_log" | tail -n 1)
deviations=$(sed -n 's/.*deviations=\([0-9][0-9]*\).*/\1/p' "$last_log" | tail -n 1)

{
    echo "K456/K462 syscall robustness measurement"
    echo "arch=$arch mode=$mode smp=$smp kvm=$kvm_state"
    echo "commit=$(git rev-parse HEAD)"
    echo "qemu=$(qemu-system-$arch --version | head -n 1)"
    echo "case=$case_file image=$image seed=0x5a17c0de12349876"
    echo "command=sh tests/smp/syscall-fuzz/run.sh $mode --iterations $iterations --arch $arch"
    echo "iterations=$iterations boots=$((iterations * 2)) determinism=identical-hash"
    echo "table-hash=$(cat "$out"/run-*/hash.txt | tail -n 1)"
    echo "rows=$rows deviations=$deviations"
} | tee "$out/summary.txt"
