#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S107 KVM/TCG release-image boot-loop soak.
#
# Each boot is a foreground QEMU process. Its serial output is tee'd to a raw log and a
# timestamped log; the runner requires `sched: user SMP on`, `smp: N cpus online` and a
# `login:` prompt, preserves every log, and stops on the first failure without retrying.
# The soak passes no boot flags, so `sched: user SMP on` is the shipped-default contract.
# The timestamp of the first login line gives boot-to-login; the per-mode p50/p90 are
# written to summary.txt.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd -P)
cd "$root"

image=build/x86_64/ravenbsd-x86_64.img
profile=release
kvm4=50
kvm2=20
kvm1=20
tcg4=10
kvm_timeout=15
tcg_timeout=30
force_build=0
wait_idle=0
default_max_load() {
    n=$(nproc 2>/dev/null || echo 4)
    case "$n" in ''|*[!0-9]*) n=4 ;; esac
    m=$((n / 4))
    [ "$m" -lt 2 ] && m=2
    echo "$m"
}
max_load=$(default_max_load)
out_root=build/smp-boot-soak

# Per-boot host telemetry (S107 rev 49). A sample is best-effort and never changes a boot's
# pass/fail, timeout or login detection. `jobs` is the live xtask/cargo/rustc/clippy/rustdoc
# class; zombie, dead, stopped and traced processes are ignored, matching the preflight.
host_sample() {
    load1=$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
    counts=$(ps -eo stat=,comm= 2>/dev/null | awk '
        $1 !~ /^[ZXTt]/ {
            if ($2 ~ /^qemu-system-/) q++
            else if ($2 ~ /^(xtask|cargo|rustc|clippy-driver|rustdoc)$/) j++
        }
        END { printf "%d %d", q+0, j+0 }')
    printf '%s %s\n' "$load1" "$counts"
}

record_host_row() {
    # mode run start end before after status login_s
    _mode=$1; _run=$2; _start=$3; _end=$4; _before=$5; _after=$6; _status=$7; _login=${8:-}
    _lb=${_before%% *}; _rest=${_before#* }; _qb=${_rest%% *}; _jb=${_rest#* }
    _la=${_after%% *}; _rest=${_after#* }; _qa=${_rest%% *}; _ja=${_rest#* }
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$_mode" "$_run" "$_start" "$_end" "$_lb" "$_qb" "$_jb" "$_la" "$_qa" "$_ja" "$_status" "$_login" \
        >> "$host_tsv" 2>/dev/null || true
}

print_host_summary() {
    [ -f "$host_tsv" ] || return 0
    _rows=$(awk 'END { print NR }' "$host_tsv" 2>/dev/null || echo 1)
    case "$_rows" in ''|*[!0-9]*) _rows=1 ;; esac
    _rows=$((_rows - 1))
    [ "$_rows" -lt 0 ] && _rows=0
    _tab=$(printf '\t')
    awk -F "$_tab" -v rows="$_rows" -v file="$host_tsv" '
        NR > 1 {
            if ($5+0 > lb) lb=$5+0; if ($8+0 > la) la=$8+0;
            if ($6+0 > qb) qb=$6+0; if ($9+0 > qa) qa=$9+0;
            if ($7+0 > jb) jb=$7+0; if ($10+0 > ja) ja=$10+0;
        }
        END {
            printf "smp-boot-soak: host telemetry boots=%d max_load1_before=%d max_load1_after=%d max_qemu_before=%d max_qemu_after=%d max_jobs_before=%d max_jobs_after=%d file=%s\n", rows, lb, la, qb, qa, jb, ja, file
        }' "$host_tsv" 2>/dev/null || echo "smp-boot-soak: host telemetry boots=$_rows file=$host_tsv"
}

usage() {
    cat <<'EOF'
usage: sh scripts/smp-boot-soak.sh [options]

  --kvm4 N        number of KVM -smp 4 boots (default 50)
  --kvm2 N        number of KVM -smp 2 boots (default 20)
  --kvm1 N        number of KVM -smp 1 boots (default 20)
  --tcg4 N        number of TCG -smp 4 boots (default 10)
  --timeout-kvm S per-boot KVM timeout (default 15)
  --timeout-tcg S per-boot TCG timeout (default 30)
  --image PATH    release image (default build/x86_64/ravenbsd-x86_64.img)
  --build         force a release image build (needs a clean tree)
  --out DIR       log root (default build/smp-boot-soak/<utc>)
  --wait-for-idle S wait up to S seconds for no other QEMU/xtask/cargo/rustc and load <= 2.0
  --max-load L    maximum 1-minute load average for the preflight (default max(2, nproc/4))
  -h, --help      show this help

The release image is built only when missing or --build is given. Every boot is foreground;
a missing `sched: user SMP on`, `smp: N cpus online` or `login:` line is a failure. The
preflight refuses a busy host by default; use --wait-for-idle on a shared machine. KVM aarch64
is not available on this host; the runner is x86_64 only.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --kvm4) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; kvm4=$2; shift 2 ;;
        --kvm2) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; kvm2=$2; shift 2 ;;
        --kvm1) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; kvm1=$2; shift 2 ;;
        --tcg4) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; tcg4=$2; shift 2 ;;
        --timeout-kvm) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; kvm_timeout=$2; shift 2 ;;
        --timeout-tcg) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; tcg_timeout=$2; shift 2 ;;
        --image) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; image=$2; shift 2 ;;
        --build) force_build=1; shift ;;
        --out) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; out_root=$2; shift 2 ;;
        --wait-for-idle) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; wait_idle=$2; shift 2 ;;
        --max-load) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; max_load=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "smp-boot-soak: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

stamp=$(date -u +%Y%m%dT%H%M%SZ)
out=$out_root/$stamp
mkdir -p "$out"
host_tsv=$out/host.tsv
printf 'mode\trun\tstart_utc\tend_utc\tload1_before\tqemu_before\tjobs_before\tload1_after\tqemu_after\tjobs_after\tstatus\tlogin_s\n' > "$host_tsv"
trap print_host_summary EXIT

sh tests/smp/boot/host-idle.sh \
    --context "smp-boot-soak" \
    --wait "$wait_idle" \
    --max-load "$max_load" \
    --state-file "$out/host-state.txt"

if [ "$force_build" -eq 1 ] || [ ! -f "$image" ]; then
    cargo xtask image --arch x86_64 --profile "$profile"
fi
if [ ! -f "$image" ]; then
    echo "smp-boot-soak: image not found: $image" >&2
    exit 2
fi
if [ ! -w /dev/kvm ] && { [ "$kvm4" -gt 0 ] || [ "$kvm2" -gt 0 ] || [ "$kvm1" -gt 0 ]; }; then
    echo "smp-boot-soak: BLOCKED: /dev/kvm is not readable/writable" >&2
    exit 2
fi

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$code" ] || { echo "smp-boot-soak: missing OVMF code $code" >&2; exit 2; }
[ -f "$vars" ] || { echo "smp-boot-soak: missing OVMF vars $vars" >&2; exit 2; }

echo "smp-boot-soak: image=$image commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
echo "smp-boot-soak: logs=$out"
echo "smp-boot-soak: counts kvm4=$kvm4 kvm2=$kvm2 kvm1=$kvm1 tcg4=$tcg4"

run_one() {
    mode=$1
    smp=$2
    kvm=$3
    timeout_secs=$4
    count=$5
    logdir=$out/$mode
    mkdir -p "$logdir"
    durations=$logdir/durations.txt
    : > "$durations"

    i=1
    while [ "$i" -le "$count" ]; do
        raw=$logdir/run-$i.log
        stamped=$logdir/run-$i.stamped.log
        work=$(mktemp -d "${TMPDIR:-/tmp}/smp-boot-soak.XXXXXX")
        cp "$vars" "$work/vars.fd"
        if [ "$kvm" -eq 1 ]; then
            accel='-enable-kvm -cpu host'
        else
            accel=
        fi
        echo "smp-boot-soak: $mode run $i/$count smp=$smp timeout=${timeout_secs}s"
        host_start=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        host_before=$(host_sample)
        # The helper is a foreground QEMU supervisor: it reads the serial pty, timestamps lines,
        # and terminates QEMU as soon as the login prompt arrives (or at the timeout).
        if login_out=$(python3 tests/smp/boot/soak_boot.py \
            --timeout "$timeout_secs" --raw "$raw" --stamped "$stamped" -- \
            qemu-system-x86_64 \
                -machine q35 -m 512M -smp "$smp" $accel \
                -serial stdio -display none -monitor none -no-reboot \
                -drive "if=pflash,format=raw,readonly=on,file=$code" \
                -drive "if=pflash,format=raw,file=$work/vars.fd" \
                -drive "format=raw,file=$image" \
                -nic none -netdev user,id=n0 \
                -device virtio-net-pci,netdev=n0,disable-legacy=on \
                -device isa-debug-exit,iobase=0xf4,iosize=0x04); then
            host_end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
            host_after=$(host_sample)
            rm -rf "$work"
        else
            host_end=$(date -u +%Y-%m-%dT%H:%M:%SZ)
            host_after=$(host_sample)
            rm -rf "$work"
            record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
            echo "smp-boot-soak: FAIL $mode run $i: $login_out" >&2
            echo "smp-boot-soak: log=$raw" >&2
            tail -n 40 "$raw" >&2 || true
            exit 1
        fi

        if ! grep -q "smp: $smp cpus online" "$raw"; then
            record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
            echo "smp-boot-soak: FAIL $mode run $i: no 'smp: $smp cpus online'" >&2
            echo "smp-boot-soak: log=$raw" >&2
            tail -n 40 "$raw" >&2
            exit 1
        fi
        if ! grep -q 'sched: user SMP on' "$raw"; then
            record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
            echo "smp-boot-soak: FAIL $mode run $i: no 'sched: user SMP on' (default user SMP is off)" >&2
            echo "smp-boot-soak: log=$raw" >&2
            tail -n 40 "$raw" >&2
            exit 1
        fi
        if ! grep -q 'login:' "$raw"; then
            record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
            echo "smp-boot-soak: FAIL $mode run $i: no login prompt" >&2
            echo "smp-boot-soak: log=$raw" >&2
            tail -n 40 "$raw" >&2
            exit 1
        fi
        if grep -qiE 'CPU Reset|triple fault|KERNEL PANIC' "$raw"; then
            record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
            echo "smp-boot-soak: FAIL $mode run $i: reset/panic marker" >&2
            echo "smp-boot-soak: log=$raw" >&2
            tail -n 40 "$raw" >&2
            exit 1
        fi

        elapsed=${login_out#login_s=}
        case $elapsed in
            ''|*[!0-9.]*)
                record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" fail ""
                echo "smp-boot-soak: FAIL $mode run $i: bad timing '$login_out'" >&2
                exit 1
                ;;
        esac
        record_host_row "$mode" "$i" "$host_start" "$host_end" "$host_before" "$host_after" pass "$elapsed"
        printf '%s\n' "$elapsed" >> "$durations"
        echo "smp-boot-soak: PASS $mode run $i login_s=$elapsed"
        i=$((i + 1))
    done

    count_actual=$(wc -l < "$durations" | tr -d ' ')
    if [ "$count_actual" -eq 0 ]; then
        return 0
    fi
    sort -n "$durations" > "$logdir/durations.sorted"
    worst=$(tail -n 1 "$logdir/durations.sorted")
    p50_rank=$(awk -v n="$count_actual" 'BEGIN { print int((n * 50 + 99) / 100) }')
    p90_rank=$(awk -v n="$count_actual" 'BEGIN { print int((n * 90 + 99) / 100) }')
    p50=$(sed -n "${p50_rank}p" "$logdir/durations.sorted")
    p90=$(sed -n "${p90_rank}p" "$logdir/durations.sorted")
    printf '%s runs=%s p50_s=%s p90_s=%s worst_s=%s\n' "$mode" "$count_actual" "$p50" "$p90" "$worst" >> "$out/summary.txt"
    echo "smp-boot-soak: $mode runs=$count_actual p50_s=$p50 p90_s=$p90 worst_s=$worst"
}

: > "$out/summary.txt"
if [ "$kvm1" -gt 0 ]; then run_one kvm1 1 1 "$kvm_timeout" "$kvm1"; fi
if [ "$kvm2" -gt 0 ]; then run_one kvm2 2 1 "$kvm_timeout" "$kvm2"; fi
if [ "$kvm4" -gt 0 ]; then run_one kvm4 4 1 "$kvm_timeout" "$kvm4"; fi
if [ "$tcg4" -gt 0 ]; then run_one tcg4 4 0 "$tcg_timeout" "$tcg4"; fi

echo "smp-boot-soak: PASS all requested boots"
echo "smp-boot-soak: summary=$out/summary.txt"
cat "$out/summary.txt"
