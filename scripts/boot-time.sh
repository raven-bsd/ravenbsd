#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Boot-to-login timing for the x86_64 release image.
#
# The OS gate is the first `RavenBSD kernel` banner -> `login:` prompt, measured over three
# runs. QEMU process start -> login is reported separately because OVMF dominates it. The image
# is built from the current commit (or supplied with --image) and the guest banner must carry
# that commit; a reused image from another commit is refused.
#
# KVM 4-CPU is the 0.1.0 gate after S83. Until S83 lands this script runs the 1-CPU KVM baseline
# and prints the 4-CPU gate as pending; it never invents a 4-CPU result. --tcg runs the same
# measurement without KVM and reports it without the gate.
#
# Usage:
#   sh scripts/boot-time.sh                  # KVM 1 CPU, 3 runs
#   sh scripts/boot-time.sh --smp 4          # after S83; KVM 4-CPU gate
#   sh scripts/boot-time.sh --tcg            # TCG report, no gate
#   sh scripts/boot-time.sh --image FILE     # use an existing image (commit checked)
#   sh scripts/boot-time.sh --runs 1         # shorter exploratory run
#
# No QEMU/OVMF or user input is sent; the VM is stopped as soon as the login prompt appears.

set -eu

arch=x86_64
runs=3
smp=1
kvm=1
timeout_s=300
image=
out=build/boot-time
ovmf_code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
ovmf_vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
qemu_pid=
tmpdir=

usage() {
    cat <<'EOF'
usage: sh scripts/boot-time.sh [--arch x86_64] [--smp N] [--runs N] [--tcg]
                               [--image FILE] [--out DIR] [--timeout SECONDS]

  --arch x86_64  only the x86_64 release gate is supported.
  --smp N        QEMU CPU count (default 1; the gate is 4 after S83).
  --runs N       measured runs (default 3).
  --tcg          disable KVM; report only, no gate.
  --image FILE   use an existing image instead of building one; its banner commit is checked.
  --out DIR      serial logs and summary (default build/boot-time).
  --timeout N    per-run wall-clock limit (default 300 s).
EOF
}

fail() {
    printf 'boot-time: FAIL %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "${qemu_pid:-}" ]; then
        kill "$qemu_pid" 2>/dev/null || true
        wait "$qemu_pid" 2>/dev/null || true
    fi
    [ -n "${tmpdir:-}" ] && rm -rf "$tmpdir"
}

trap cleanup EXIT HUP INT TERM

while [ "$#" -gt 0 ]; do
    case $1 in
        --arch)
            [ "$#" -ge 2 ] || { printf 'boot-time: --arch needs a value\n' >&2; exit 2; }
            arch=$2
            shift 2
            ;;
        --smp)
            [ "$#" -ge 2 ] || { printf 'boot-time: --smp needs a value\n' >&2; exit 2; }
            smp=$2
            shift 2
            ;;
        --runs)
            [ "$#" -ge 2 ] || { printf 'boot-time: --runs needs a value\n' >&2; exit 2; }
            runs=$2
            shift 2
            ;;
        --timeout)
            [ "$#" -ge 2 ] || { printf 'boot-time: --timeout needs a value\n' >&2; exit 2; }
            timeout_s=$2
            shift 2
            ;;
        --image)
            [ "$#" -ge 2 ] || { printf 'boot-time: --image needs a value\n' >&2; exit 2; }
            image=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { printf 'boot-time: --out needs a value\n' >&2; exit 2; }
            out=$2
            shift 2
            ;;
        --tcg)
            kvm=0
            shift
            ;;
        --help | -h)
            usage
            exit 0
            ;;
        *)
            printf 'boot-time: unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

[ "$arch" = "x86_64" ] || {
    printf 'boot-time: only the x86_64 release gate is supported (got %s)\n' "$arch" >&2
    exit 2
}
case $smp in
    '' | *[!0-9]*) printf 'boot-time: invalid --smp value: %s\n' "$smp" >&2; exit 2 ;;
esac
case $runs in
    '' | *[!0-9]*) printf 'boot-time: invalid --runs value: %s\n' "$runs" >&2; exit 2 ;;
esac
[ "$runs" -ge 1 ] || { printf 'boot-time: --runs must be at least 1\n' >&2; exit 2; }
case $timeout_s in
    '' | *[!0-9]*) printf 'boot-time: invalid --timeout value: %s\n' "$timeout_s" >&2; exit 2 ;;
esac

[ -r "$ovmf_code" ] || fail "OVMF code not found: $ovmf_code (set RAVEN_OVMF_CODE)"
[ -r "$ovmf_vars" ] || fail "OVMF vars not found: $ovmf_vars (set RAVEN_OVMF_VARS)"
command -v qemu-system-x86_64 >/dev/null 2>&1 || fail "qemu-system-x86_64 is not installed"
if [ "$kvm" -eq 1 ]; then
    [ -r /dev/kvm ] || fail "/dev/kvm is not readable; use --tcg for a software run"
fi

commit=$(git rev-parse --short HEAD)
[ -n "$commit" ] || fail "cannot read HEAD"

mkdir -p "$out"
if [ -z "$image" ]; then
    image=$out/release.img
    printf '==> build release image from %s\n' "$commit"
    if ! cargo xtask image --arch x86_64 --platform generic --profile release \
        --output "$image" >"$out/build.log" 2>&1; then
        tail -20 "$out/build.log" >&2
        fail "release image build failed"
    fi
else
    [ -f "$image" ] || fail "image not found: $image"
fi
[ -f "$image" ] || fail "image not found: $image"
image_sha=$(sha256sum "$image" | awk '{print $1}')

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/boot-time.XXXXXX")
kernel_file=$tmpdir/kernel-ms
total_file=$tmpdir/total-ms
firmware_file=$tmpdir/firmware-ms
: >"$kernel_file"
: >"$total_file"
: >"$firmware_file"

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

run=1
while [ "$run" -le "$runs" ]; do
    serial=$tmpdir/serial-$run.log
    vars=$tmpdir/vars-$run.fd
    cp "$ovmf_vars" "$vars"
    : >"$serial"

    start_ns=$(date +%s%N)
    if [ "$kvm" -eq 1 ]; then
        timeout "$timeout_s" qemu-system-x86_64 \
            -machine q35 -m 512M -smp "$smp" -serial "file:$serial" \
            -display none -monitor none -no-reboot \
            -enable-kvm -cpu host \
            -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code" \
            -drive "if=pflash,format=raw,file=$vars" \
            -drive "format=raw,file=$image" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            </dev/null >/dev/null 2>&1 &
    else
        timeout "$timeout_s" qemu-system-x86_64 \
            -machine q35 -m 512M -smp "$smp" -serial "file:$serial" \
            -display none -monitor none -no-reboot \
            -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code" \
            -drive "if=pflash,format=raw,file=$vars" \
            -drive "format=raw,file=$image" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            </dev/null >/dev/null 2>&1 &
    fi
    qemu_pid=$!

    kernel_ns=
    login_ns=
    kernel_line=
    deadline_ns=$((start_ns + timeout_s * 1000000000))
    while :; do
        if [ -z "$kernel_ns" ] && grep -aqE 'RavenBSD kernel 0\.1 \(x86_64\) [0-9a-f]+ [0-9-]+' "$serial" 2>/dev/null; then
            kernel_ns=$(date +%s%N)
            kernel_line=$(grep -aE 'RavenBSD kernel 0\.1 \(x86_64\) [0-9a-f]+ [0-9-]+' "$serial" | head -1)
        fi
        if grep -aq 'login:' "$serial" 2>/dev/null; then
            login_ns=$(date +%s%N)
            break
        fi
        if ! kill -0 "$qemu_pid" 2>/dev/null; then
            break
        fi
        now_ns=$(date +%s%N)
        [ "$now_ns" -lt "$deadline_ns" ] || break
        sleep 0.005
    done

    kill "$qemu_pid" 2>/dev/null || true
    wait "$qemu_pid" 2>/dev/null || true
    qemu_pid=
    cp "$serial" "$out/run-$run.log"

    [ -n "$kernel_ns" ] || {
        tail -20 "$serial" >&2
        fail "run $run: no RavenBSD kernel banner within ${timeout_s}s (log $out/run-$run.log)"
    }
    [ -n "$login_ns" ] || {
        tail -20 "$serial" >&2
        fail "run $run: no login prompt within ${timeout_s}s (log $out/run-$run.log)"
    }
    case $kernel_line in
        *"$commit"*) ;;
        *)
            fail "run $run: guest kernel id does not match HEAD $commit: $kernel_line"
            ;;
    esac

    firmware_ms=$(((kernel_ns - start_ns) / 1000000))
    kernel_ms=$(((login_ns - kernel_ns) / 1000000))
    total_ms=$(((login_ns - start_ns) / 1000000))
    printf 'boot-time: run=%s firmware_ms=%s kernel_ms=%s total_ms=%s kernel_id=%s\n' \
        "$run" "$firmware_ms" "$kernel_ms" "$total_ms" "$commit"
    printf '%s\n' "$firmware_ms" >>"$firmware_file"
    printf '%s\n' "$kernel_ms" >>"$kernel_file"
    printf '%s\n' "$total_ms" >>"$total_file"
    run=$((run + 1))
done

median_firmware=$(median "$firmware_file")
median_kernel=$(median "$kernel_file")
median_total=$(median "$total_file")
p90_firmware=$(p90 "$firmware_file")
p90_kernel=$(p90 "$kernel_file")
p90_total=$(p90 "$total_file")

if [ "$kvm" -eq 1 ]; then accel=kvm; else accel=tcg; fi
printf 'boot-time: commit=%s arch=%s accel=%s smp=%s runs=%s image=%s sha256=%s\n' \
    "$commit" "$arch" "$accel" "$smp" "$runs" "$image" "$image_sha"
printf 'boot-time: median firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
    "$median_firmware" "$median_kernel" "$median_total"
printf 'boot-time: p90 firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
    "$p90_firmware" "$p90_kernel" "$p90_total"

if [ "$kvm" -eq 1 ] && [ "$smp" -eq 4 ]; then
    if [ "$median_kernel" -lt 1000 ]; then
        printf 'boot-time: GATE PASS: KVM 4-CPU kernel->login median %sms < 1000ms\n' "$median_kernel"
    else
        printf 'boot-time: GATE FAIL: KVM 4-CPU kernel->login median %sms >= 1000ms\n' "$median_kernel" >&2
        exit 1
    fi
elif [ "$kvm" -eq 1 ]; then
    printf 'boot-time: GATE PENDING (S83 4-CPU): 1-CPU KVM baseline kernel->login median %sms\n' "$median_kernel"
else
    printf 'boot-time: GATE NOT APPLICABLE (TCG): kernel->login median %sms reported only\n' "$median_kernel"
fi
