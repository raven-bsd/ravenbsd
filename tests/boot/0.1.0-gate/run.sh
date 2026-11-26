#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# 0.1.0 release boot-time gate: QEMU start -> getty `login:` marker, firmware included.
#
# Boots the release profile with the requested architecture and accelerator, five times by
# default, and fails a run that misses the login marker, prints a panic, times out, or takes
# 1000 ms or more. The x86_64 cell uses the S89 PVH direct release boot (kernel + release
# initfs); the aarch64 cell uses the release image under AAVMF. Both start the QEMU clock
# before the process spawn, so firmware time is part of the measurement.
#
# Usage:
#   sh tests/boot/0.1.0-gate/run.sh --arch x86_64 --accel kvm --runs 5
#   sh tests/boot/0.1.0-gate/run.sh --arch x86_64 --accel tcg --runs 5
#   sh tests/boot/0.1.0-gate/run.sh --arch aarch64 --accel tcg --runs 5
#   sh tests/boot/0.1.0-gate/run.sh --mutation fake-login-marker
#   sh tests/boot/0.1.0-gate/run.sh --mutation fixed-timestamp
#
# The runner writes serial transcripts and the summary under --out (default
# build/boot-gate/<arch>-<accel>). Evidence copies belong in the prjmgmt ticket artifacts,
# never in this product tree.
#
# Exit: 0 when every run is clean and the maximum is under 1000 ms; 1 when the cell fails its
# gate or a mutation was not caught; 2 for usage, build or infrastructure errors.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
cd "$root"

arch=x86_64
accel=kvm
runs=5
smp=4
timeout_s=90
out=
mutation=

usage() {
    sed -n 's/^# \{0,1\}//p' "$0" | sed -n '/^Usage:/,/^Exit:/p'
}

refuse() {
    printf 'boot-gate: %s\n' "$*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) arch=${2:?--arch needs a value}; shift 2 ;;
        --accel) accel=${2:?--accel needs a value}; shift 2 ;;
        --runs) runs=${2:?--runs needs a value}; shift 2 ;;
        --smp) smp=${2:?--smp needs a value}; shift 2 ;;
        --timeout) timeout_s=${2:?--timeout needs a value}; shift 2 ;;
        --out) out=${2:?--out needs a value}; shift 2 ;;
        --mutation) mutation=${2:?--mutation needs a value}; shift 2 ;;
        --help | -h) usage; exit 0 ;;
        *) usage >&2; refuse "unknown argument: $1" ;;
    esac
done

case "$arch" in
    x86_64 | aarch64) ;;
    *) refuse "unsupported --arch: $arch (expected x86_64 or aarch64)" ;;
esac
case "$accel" in
    kvm | tcg) ;;
    *) refuse "unsupported --accel: $accel (expected kvm or tcg)" ;;
esac
if [ "$arch" = aarch64 ] && [ "$accel" = kvm ]; then
    refuse "aarch64 KVM is not a supported QEMU gate cell; use --accel tcg"
fi
case "$runs" in '' | *[!0-9]*) refuse "invalid --runs: $runs" ;; esac
case "$smp" in '' | *[!0-9]*) refuse "invalid --smp: $smp" ;; esac
[ "$runs" -ge 1 ] || refuse "--runs must be at least 1"
[ "$smp" -ge 1 ] || refuse "--smp must be at least 1"
case "$timeout_s" in '' | *[!0-9]*) refuse "invalid --timeout: $timeout_s" ;; esac
[ "$timeout_s" -ge 1 ] || refuse "--timeout must be at least 1"

measure=$here/measure.py
fake=$here/fake_qemu.py
[ -r "$measure" ] || refuse "measurement helper not found: $measure"
[ -r "$fake" ] || refuse "mutation helper not found: $fake"

if [ -n "$mutation" ]; then
    case "$mutation" in
        fake-login-marker) scenario=marker-only ;;
        fixed-timestamp) scenario=fixed-timestamp ;;
        *) refuse "unknown --mutation: $mutation (expected fake-login-marker or fixed-timestamp)" ;;
    esac
    out=${out:-build/boot-gate/mutation-$mutation}
    mkdir -p "$out"
    log=$out/run-1.log
    result=0
    output=$(timeout $((timeout_s + 15)) python3 "$measure" --log "$log" \
        --timeout "$timeout_s" -- python3 "$fake" --scenario "$scenario" \
        --serial @SERIAL@) || result=$?
    if [ "$result" -eq 0 ]; then
        printf 'mutation: NOT CAUGHT: the gate accepted the %s stream (log %s)\n' \
            "$mutation" "$log"
        exit 0
    fi
    if [ "$mutation" = fixed-timestamp ]; then
        measured=$(printf '%s\n' "$output" | sed -n 's/^boot_ms=//p')
        case "$measured" in
            '' | *[!0-9]*) refuse "mutation produced no boot_ms (log $log)" ;;
        esac
        [ "$measured" -ge 1000 ] ||
            refuse "mutation measured ${measured} ms; the forged timestamp was trusted (log $log)"
        printf 'mutation: forged boot_ms=1 ignored; host clock measured %s ms, gate rejected it\n' \
            "$measured"
    else
        printf 'mutation: fake login marker rejected by the kernel-banner check\n'
    fi
    printf 'mutation: %s caught (exit %s, log %s)\n' "$mutation" "$result" "$log"
    exit 1
fi

[ -z "$(git status --porcelain)" ] ||
    refuse "worktree is dirty; a release measurement must bind the kernel build id to HEAD"
commit=$(git rev-parse --short HEAD)
[ -n "$commit" ] || refuse "cannot read HEAD"

out=${out:-build/boot-gate/$arch-$accel}
mkdir -p "$out"
build_log=$out/build.log
artifact=$out/artifacts.tsv
host_state=$out/host.txt
: >"$build_log"
: >"$artifact"
awk '{print "load1=" $1, "load5=" $2, "load15=" $3}' /proc/loadavg >"$host_state"
printf 'nproc=%s\n' "$(nproc)" >>"$host_state"

fingerprint() {
    [ -f "$1" ] || refuse "measured artifact not found: $1"
    printf '%s\t%s\n' "$(sha256sum "$1" | awk '{print $1}')" "$1" >>"$artifact"
}

case "$arch" in
    x86_64)
        command -v qemu-system-x86_64 >/dev/null 2>&1 ||
            refuse "qemu-system-x86_64 is not installed"
        if [ "$accel" = kvm ] && { [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; }; then
            refuse "/dev/kvm is not readable and writable; use --accel tcg"
        fi
        cargo xtask build kernel --arch x86_64 --release >>"$build_log" 2>&1 ||
            refuse "release kernel build failed (see $build_log)"
        cargo xtask initfs --arch x86_64 --profile release >>"$build_log" 2>&1 ||
            refuse "release initfs build failed (see $build_log)"
        kernel=$root/build/x86_64/kernel.elf
        initfs=$root/build/x86_64/initfs.img
        loader=$out/raven-direct.elf
        payload=$out/raven-direct-payload.bin
        loader_ld=$root/boot/direct/loader.ld
        [ -f "$kernel" ] || refuse "kernel not found: $kernel"
        [ -f "$initfs" ] || refuse "initfs not found: $initfs"
        RUSTFLAGS="-C relocation-model=static -C link-arg=-T$loader_ld -C link-arg=--build-id=none" \
            cargo build -p raven-direct --bin raven-direct --release \
            --target x86_64-unknown-none --target-dir "$root/target" --features loader \
            >>"$build_log" 2>&1 || refuse "direct loader build failed (see $build_log)"
        cp "$root/target/x86_64-unknown-none/release/raven-direct" "$loader"
        # Same PVH payload contract as scripts/boot-direct.sh: header, kernel ELF padded so the
        # initfs starts page-aligned, then the release initfs.
        python3 - "$kernel" "$initfs" "$payload" <<'PY' || refuse "direct payload assembly failed"
import struct
import sys

with open(sys.argv[1], "rb") as source:
    kernel = source.read()
with open(sys.argv[2], "rb") as source:
    initfs = source.read()
padded_kernel = ((len(kernel) + 16 + 4095) // 4096) * 4096 - 16
if padded_kernel < len(kernel):
    raise SystemExit("boot-gate: kernel padding underflow")
with open(sys.argv[3], "wb") as output:
    output.write(b"RVDIRECT")
    output.write(struct.pack("<Q", padded_kernel))
    output.write(kernel)
    output.write(b"\0" * (padded_kernel - len(kernel)))
    output.write(initfs)
PY
        fingerprint "$kernel"
        fingerprint "$initfs"
        fingerprint "$loader"
        fingerprint "$payload"
        ;;
    aarch64)
        command -v qemu-system-aarch64 >/dev/null 2>&1 ||
            refuse "qemu-system-aarch64 is not installed"
        ovmf_code=${RAVEN_AAVMF_CODE:-/usr/share/AAVMF/AAVMF_CODE.fd}
        ovmf_vars=${RAVEN_AAVMF_VARS:-/usr/share/AAVMF/AAVMF_VARS.fd}
        [ -r "$ovmf_code" ] || refuse "AAVMF code not found: $ovmf_code"
        [ -r "$ovmf_vars" ] || refuse "AAVMF vars not found: $ovmf_vars"
        image=$out/aarch64-release.img
        RAVEN_QT_ARCH=$arch cargo xtask image --arch aarch64 --platform generic --profile release \
            --cmdline "console=ttyAMA0 test-exit=1" --output "$image" >>"$build_log" 2>&1 ||
            refuse "aarch64 release image build failed (see $build_log)"
        fingerprint "$image"
        ;;
esac

summary=$out/summary.tsv
printf 'run\tboot_ms\tkernel_ms\texit\tlog\n' >"$summary"
boots=$out/boot-ms.txt
: >"$boots"
clean=0
failed=0
run=1
while [ "$run" -le "$runs" ]; do
    log=$out/run-$run.log
    result=0
    if [ "$arch" = x86_64 ]; then
        if [ "$accel" = kvm ]; then
            accel_args="-enable-kvm -cpu host"
        else
            accel_args=
        fi
        # shellcheck disable=SC2086
        output=$(timeout $((timeout_s + 15)) python3 "$measure" --log "$log" --timeout "$timeout_s" -- \
            qemu-system-x86_64 -no-user-config -machine q35 -nodefaults $accel_args \
            -m 512M -smp "$smp" -serial @SERIAL@ -display none -monitor none -no-reboot \
            -kernel "$loader" -initrd "$payload" -append "console=ttyS0 test-exit=1" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            -nic none -netdev user,id=n0 -device virtio-net-pci,netdev=n0,disable-legacy=on) ||
            result=$?
    else
        vars=$out/vars-$run.fd
        cp "$ovmf_vars" "$vars"
        output=$(timeout $((timeout_s + 15)) python3 "$measure" --log "$log" --timeout "$timeout_s" -- \
            qemu-system-aarch64 -M virt,gic-version=2 -cpu cortex-a72 -m 512M -smp "$smp" \
            -serial @SERIAL@ -display none -monitor none -no-reboot \
            -semihosting-config enable=on,target=native \
            -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code" \
            -drive "if=pflash,format=raw,file=$vars" \
            -drive "format=raw,file=$image") || result=$?
    fi

    boot_ms=$(printf '%s\n' "$output" | sed -n 's/^boot_ms=//p')
    kernel_ms=$(printf '%s\n' "$output" | sed -n 's/^kernel_ms=//p')
    kernel_id=$(printf '%s\n' "$output" | sed -n 's/^kernel_id=//p')
    if [ "$result" -eq 0 ] && [ "$kernel_id" != "$commit" ]; then
        printf 'boot-gate: run %s kernel id %s does not match HEAD %s\n' \
            "$run" "$kernel_id" "$commit" >&2
        result=1
    fi
    case "$result" in
        0)
            clean=$((clean + 1))
            printf '%s\n' "$boot_ms" >>"$boots"
            printf 'boot-gate: run=%s boot_ms=%s kernel_ms=%s kernel_id=%s\n' \
                "$run" "$boot_ms" "$kernel_ms" "$kernel_id"
            printf '%s\t%s\t%s\t0\t%s\n' "$run" "$boot_ms" "$kernel_ms" "$log" >>"$summary"
            ;;
        *)
            failed=$((failed + 1))
            case "$boot_ms" in
                '' | *[!0-9]*) ;;
                *) printf '%s\n' "$boot_ms" >>"$boots" ;;
            esac
            printf 'boot-gate: run=%s FAILED exit=%s boot_ms=%s log=%s\n' \
                "$run" "$result" "${boot_ms:-none}" "$log" >&2
            [ -z "$output" ] || printf '%s\n' "$output" >&2
            printf '%s\t%s\t%s\t%s\t%s\n' "$run" "$boot_ms" "$kernel_ms" "$result" "$log" >>"$summary"
            ;;
    esac
    run=$((run + 1))
done

count=$(awk 'NF {n++} END {print n+0}' "$boots")
if [ "$clean" -eq 0 ]; then
    printf 'boot-gate: GATE FAIL: no clean run reached the login marker under 1000 ms (%s attempted)\n' \
        "$runs" >&2
fi
if [ "$count" -eq 0 ]; then
    printf 'boot-gate: GATE FAIL: no run reported a boot time (%s attempted)\n' "$runs" >&2
    exit 1
fi
min=$(sort -n "$boots" | head -n 1)
max=$(sort -n "$boots" | tail -n 1)
median=$(sort -n "$boots" | awk '{v[NR]=$1} END {
    if (NR % 2) print v[(NR + 1) / 2]
    else print int((v[NR / 2] + v[NR / 2 + 1]) / 2)
}')
seconds() {
    awk -v ms="$1" 'BEGIN { printf "%.3f", ms / 1000 }'
}

printf 'boot-gate: commit=%s arch=%s accel=%s smp=%s runs=%s clean=%s failed=%s measured=%s artifacts=%s\n' \
    "$commit" "$arch" "$accel" "$smp" "$runs" "$clean" "$failed" "$count" "$artifact"
printf 'boot-gate: boot_ms min=%s median=%s max=%s\n' "$min" "$median" "$max"
printf 'boot-gate: boot_s min=%s median=%s max=%s\n' \
    "$(seconds "$min")" "$(seconds "$median")" "$(seconds "$max")"

if [ "$clean" -ne "$runs" ]; then
    printf 'boot-gate: GATE FAIL: %s of %s runs failed (summary %s)\n' \
        "$failed" "$runs" "$summary" >&2
    exit 1
fi
if [ "$max" -ge 1000 ]; then
    printf 'boot-gate: GATE FAIL: max %s ms >= 1000 ms including firmware (summary %s)\n' \
        "$max" "$summary" >&2
    exit 1
fi
printf 'boot-gate: GATE PASS: %s/%s clean boots, max %s ms < 1000 ms including firmware\n' \
    "$clean" "$runs" "$max"
