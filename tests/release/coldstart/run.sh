#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K504 cold-start gate: boot the x86_64 release image N times under the host-hygiene check and
# fail when the kernel->login median exceeds the release budget.
#
# Baseline (S116b, b58c5ac97): median 558 ms, p90 830 ms.
# Budget: 1000 ms, the documented "boot to login under one second including firmware" goal.
# The gate uses the kernel->login sample (the OS gate), not QEMU/OVMF start->login.
#
# usage: sh tests/release/coldstart/run.sh [--boots N] [--budget-ms N]
#                                         [--image FILE] [--out DIR] [--preflight-only]

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

boots=20
budget_ms=1000
baseline_median=558
baseline_p90=830
smp=4
image=build/coldstart/release.img
out=build/coldstart
preflight=0

usage() {
    cat <<'EOF'
usage: sh tests/release/coldstart/run.sh [--boots N] [--budget-ms N]
                                        [--image FILE] [--out DIR] [--preflight-only]

  --boots N        cold boots to measure (default 20)
  --budget-ms N    median kernel->login budget (default 1000)
  --image FILE     release image (default build/coldstart/release.img)
  --out DIR        sample logs and summary (default build/coldstart)
  --preflight-only run the host-hygiene check and exit
EOF
}

fail() {
    printf 'coldstart: FAIL %s\n' "$*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --boots)
            [ "$#" -ge 2 ] || fail "--boots needs a value"
            boots=$2
            shift 2
            ;;
        --budget-ms)
            [ "$#" -ge 2 ] || fail "--budget-ms needs a value"
            budget_ms=$2
            shift 2
            ;;
        --image)
            [ "$#" -ge 2 ] || fail "--image needs a value"
            image=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || fail "--out needs a value"
            out=$2
            shift 2
            ;;
        --preflight-only)
            preflight=1
            shift
            ;;
        --help | -h)
            usage
            exit 0
            ;;
        *)
            printf 'coldstart: unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

case $boots in '' | *[!0-9]*) fail "invalid --boots";; esac
case $budget_ms in '' | *[!0-9]*) fail "invalid --budget-ms";; esac
[ "$boots" -ge 1 ] || fail "--boots must be at least 1"

# Host hygiene before any build or boot. A loaded host must leave the gate red, never produce a
# contaminated median.
if ! sh tests/release/coldstart/hygiene.sh; then
    printf 'coldstart: BLOCKED host-hygiene before the run; no median recorded\n' >&2
    exit 1
fi
[ "$preflight" -eq 1 ] && exit 0

[ -r /dev/kvm ] || fail "/dev/kvm is not readable; the 4-CPU gate needs KVM"

commit=$(git rev-parse --short HEAD)
stamp=$image.commit
if [ ! -f "$image" ] || [ ! -f "$stamp" ] || [ "$(cat "$stamp")" != "$commit" ]; then
    mkdir -p "$out"
    printf 'coldstart: building release image from %s\n' "$commit"
    if ! cargo xtask image --arch x86_64 --platform generic --profile release \
        --cmdline 'console=ttyS0 test-exit=1' --output "$image" >"$out/build.log" 2>&1; then
        tail -20 "$out/build.log" >&2
        fail "release image build failed"
    fi
    printf '%s\n' "$commit" >"$stamp"
fi
[ -f "$image" ] || fail "image not found: $image"

mkdir -p "$out"
table=$out/boots.tsv
printf 'boot\tload1\tqemu\tfirmware_ms\tkernel_ms\ttotal_ms\n' >"$table"
kernel_file=$out/kernel-ms.txt
: >"$kernel_file"

median() {
    sort -n "$1" | awk '{ value[NR] = $1 } END {
        if (NR == 0) exit 1
        if (NR % 2) print value[(NR + 1) / 2]
        else print int((value[NR / 2] + value[NR / 2 + 1]) / 2)
    }'
}

p90() {
    sort -n "$1" | awk '{ value[NR] = $1 } END {
        if (NR == 0) exit 1
        idx = int((9 * NR + 9) / 10)
        if (idx < 1) idx = 1
        if (idx > NR) idx = NR
        print value[idx]
    }'
}

sample=1
while [ "$sample" -le "$boots" ]; do
    hyg=$(sh tests/release/coldstart/hygiene.sh 2>&1) || {
        printf '%s\n' "$hyg" >&2
        printf 'coldstart: BLOCKED host-hygiene at boot %s; no median recorded\n' "$sample" >&2
        exit 1
    }
    load1=$(printf '%s\n' "$hyg" | sed -n 's/^coldstart-hygiene: qemu=[0-9]* load1=//p' | head -1)
    qemu=$(printf '%s\n' "$hyg" | sed -n 's/^coldstart-hygiene: qemu=\([0-9]*\).*/\1/p' | head -1)
    sample_dir=$out/sample-$sample
    rm -rf "$sample_dir"
    mkdir -p "$sample_dir"
    set +e
    sh scripts/boot-time.sh --runs 1 --smp "$smp" --image "$image" \
        --out "$sample_dir" --timeout 60 >"$sample_dir/boot-time.log" 2>&1
    rc=$?
    set -e
    line=$(grep '^boot-time: run=1 ' "$sample_dir/boot-time.log" | tail -1 || true)
    if [ -z "$line" ]; then
        tail -20 "$sample_dir/boot-time.log" >&2
        fail "boot $sample produced no timing line (exit $rc)"
    fi
    firmware_ms=$(printf '%s\n' "$line" | sed -n 's/.*firmware_ms=\([0-9]*\).*/\1/p')
    kernel_ms=$(printf '%s\n' "$line" | sed -n 's/.*kernel_ms=\([0-9]*\).*/\1/p')
    total_ms=$(printf '%s\n' "$line" | sed -n 's/.*total_ms=\([0-9]*\).*/\1/p')
    case $firmware_ms in '' | *[!0-9]*) fail "boot $sample invalid firmware_ms";; esac
    case $kernel_ms in '' | *[!0-9]*) fail "boot $sample invalid kernel_ms";; esac
    case $total_ms in '' | *[!0-9]*) fail "boot $sample invalid total_ms";; esac
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$sample" "$load1" "$qemu" "$firmware_ms" "$kernel_ms" "$total_ms" >>"$table"
    printf '%s\n' "$kernel_ms" >>"$kernel_file"
    printf 'coldstart: boot=%s load1=%s qemu=%s firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
        "$sample" "$load1" "$qemu" "$firmware_ms" "$kernel_ms" "$total_ms"
    sample=$((sample + 1))
done

median_ms=$(median "$kernel_file")
p90_ms=$(p90 "$kernel_file")
min_ms=$(sort -n "$kernel_file" | head -1)
max_ms=$(sort -n "$kernel_file" | tail -1)
printf 'coldstart: commit=%s boots=%s image=%s sha256=%s\n' \
    "$commit" "$boots" "$image" "$(sha256sum "$image" | awk '{print $1}')"
printf 'coldstart: baseline median=%s p90=%s; budget=%s\n' \
    "$baseline_median" "$baseline_p90" "$budget_ms"
printf 'coldstart: p50=%s p90=%s min=%s max=%s\n' "$median_ms" "$p90_ms" "$min_ms" "$max_ms"
if [ "$median_ms" -ge "$budget_ms" ]; then
    printf 'coldstart: FAIL median %sms >= budget %sms\n' "$median_ms" "$budget_ms" >&2
    exit 1
fi
printf 'coldstart: PASS median %sms < budget %sms (baseline %sms)\n' \
    "$median_ms" "$budget_ms" "$baseline_median"
