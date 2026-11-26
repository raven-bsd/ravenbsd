#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85l 0.1.0 KVM -smp 2 soak: boot the current release image repeatedly and
# require both `smp: N cpus online` and the login prompt in every cycle.
#
# Each cycle is one foreground QEMU boot behind tests/smp/boot/soak_boot.py.
# The supervisor stops QEMU when the login marker arrives; its exit status and
# the serial markers decide the cycle. Every serial, summary and hash is written
# below tickets/evidence/artifacts/S85l/ in ravenbsd-prjmgmt, never in the
# product tree. A failed cycle names the cycle number and preserves its serial.
#
# Usage:
#   sh tests/soak/smp2-0.1.0/run.sh --arch x86_64 --accel kvm --smp 2 --cycles 100

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
accel=kvm
smp=2
cycles=100
image=build/x86_64/ravenbsd-x86_64.img
timeout_secs=30
out_root=
work=

usage() {
    cat <<'EOF'
usage: sh tests/soak/smp2-0.1.0/run.sh [options]

  --arch ARCH       guest architecture (default: x86_64; this cell is KVM only)
  --accel ACCEL     accelerator (default: kvm)
  --smp N           guest CPUs (default: 2)
  --cycles N        number of boot-to-login cycles (default: 100)
  --image PATH      release image (default: build/x86_64/ravenbsd-x86_64.img)
  --out DIR         evidence root (default: <prjmgmt>/tickets/evidence/artifacts/S85l/runs)
  --timeout S       per-cycle serial timeout in seconds (default: 30)
  -h, --help        show this help

Each cycle is a foreground QEMU boot that must print `smp: N cpus online` and
the login prompt. A panic, timeout, missing CPU, missing login or non-zero
supervisor exit fails the run with the cycle number and its serial. RAVEN_PRJMGMT
overrides the prjmgmt checkout; RAVEN_OVMF_CODE and RAVEN_OVMF_VARS override the
firmware paths.
EOF
}

cleanup() {
    [ -z "$work" ] || rm -rf "$work"
}
trap cleanup EXIT HUP INT TERM

is_uint() {
    case "$1" in
        ''|*[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; arch=$2; shift 2 ;;
        --accel) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; accel=$2; shift 2 ;;
        --smp) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; smp=$2; shift 2 ;;
        --cycles) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; cycles=$2; shift 2 ;;
        --image) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; image=$2; shift 2 ;;
        --out) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; out_root=$2; shift 2 ;;
        --timeout) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; timeout_secs=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "s85l: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

find_prjmgmt() {
    if [ -n "${RAVEN_PRJMGMT:-}" ]; then
        printf '%s\n' "$RAVEN_PRJMGMT"
        return 0
    fi
    dir=$root
    while [ "$dir" != "/" ]; do
        if [ -d "$dir/ravenbsd-prjmgmt/tickets" ]; then
            printf '%s\n' "$dir/ravenbsd-prjmgmt"
            return 0
        fi
        dir=$(dirname "$dir")
    done
    return 1
}

if [ -z "$out_root" ]; then
    prjmgmt=$(find_prjmgmt) || {
        echo "s85l: cannot locate ravenbsd-prjmgmt; set RAVEN_PRJMGMT" >&2
        exit 2
    }
    out_root=$prjmgmt/tickets/evidence/artifacts/S85l/runs
fi
stamp=$(date -u +%Y%m%dT%H%M%SZ)
started_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
label=$(printf '%s-%s-%s-smp%s' "$stamp" "$arch" "$accel" "$smp" | tr -c 'A-Za-z0-9._-' '_')
out=$out_root/$label
mkdir -p "$out/serials"

commit=$(git rev-parse HEAD)
image_sha=missing
[ ! -f "$image" ] || image_sha=$(sha256sum "$image" | cut -d' ' -f1)
load1=$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo unknown)
nproc=$(nproc 2>/dev/null || echo unknown)

write_meta() {
    {
        printf 'ticket=S85l\n'
        printf 'arch=%s\n' "$arch"
        printf 'accel=%s\n' "$accel"
        printf 'smp=%s\n' "$smp"
        printf 'cycles_requested=%s\n' "$cycles"
        printf 'commit=%s\n' "$commit"
        printf 'image=%s\n' "$image"
        printf 'image_sha256=%s\n' "$image_sha"
        printf 'timeout_s=%s\n' "$timeout_secs"
        printf 'load1=%s\n' "$load1"
        printf 'nproc=%s\n' "$nproc"
        printf 'started_utc=%s\n' "$started_utc"
        printf 'finished_utc=%s\n' "${finished:-}"
        printf 'failed=%s\n' "${failed:-false}"
    } > "$out/run.txt"
}

write_rows() {
    {
        printf 'cycle\tstatus\tlogin_s\tserial\tsupervisor_rc\n'
        [ ! -f "$out/cycles.tsv" ] || cat "$out/cycles.tsv"
    } > "$out/summary.tsv"
}

write_hashes() {
    (
        cd "$out"
        find . -type f ! -name SHA256SUMS -print | sort | while IFS= read -r path; do
            sha256sum "${path#./}"
        done
    ) > "$out/SHA256SUMS"
}

invalid_cell() {
    reason=$1
    failed=true
    finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    write_meta
    {
        printf 'status=invalid-cell\n'
        printf 'reason=%s\n' "$reason"
    } > "$out/cell.txt"
    printf '0\tinvalid\t0\t\t126\n' > "$out/cycles.tsv"
    write_rows
    write_hashes
    echo "s85l: FAIL cell arch=$arch accel=$accel smp=$smp cycles=$cycles: $reason" >&2
    echo "s85l: evidence=$out" >&2
    exit 2
}

is_uint "$cycles" && [ "$cycles" -ge 1 ] || invalid_cell "invalid cycle count '$cycles'"
is_uint "$smp" && [ "$smp" -ge 1 ] && [ "$smp" -le 64 ] ||
    invalid_cell "impossible cpu count '$smp' (valid range 1..64)"
is_uint "$timeout_secs" && [ "$timeout_secs" -ge 1 ] || invalid_cell "invalid timeout '$timeout_secs'"
[ "$arch" = x86_64 ] || invalid_cell "unsupported architecture '$arch' for this KVM cell"
[ "$accel" = kvm ] || invalid_cell "missing accelerator '$accel' (this cell requires kvm)"
[ -w /dev/kvm ] || invalid_cell "missing accelerator: /dev/kvm is not writable"
[ -f "$image" ] || invalid_cell "image not found: $image"

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$code" ] || invalid_cell "firmware code not found: $code"
[ -f "$vars" ] || invalid_cell "firmware vars not found: $vars"
[ -f tests/smp/boot/soak_boot.py ] || invalid_cell "serial supervisor not found: tests/smp/boot/soak_boot.py"
command -v qemu-system-x86_64 >/dev/null 2>&1 || invalid_cell "missing qemu-system-x86_64"
command -v python3 >/dev/null 2>&1 || invalid_cell "missing python3"

failed=false
finished=
: > "$out/cycles.tsv"
write_meta
echo "s85l: start utc=$stamp arch=$arch accel=$accel smp=$smp cycles=$cycles commit=$commit image_sha256=$image_sha load1=$load1 nproc=$nproc evidence=$out"

cycle=1
while [ "$cycle" -le "$cycles" ]; do
    raw=$out/serials/cycle-$(printf '%03d' "$cycle").log
    stamped=$out/serials/cycle-$(printf '%03d' "$cycle").stamped.log
    supervisor_log=$out/serials/cycle-$(printf '%03d' "$cycle").supervisor.log
    work=$(mktemp -d "${TMPDIR:-/tmp}/s85l.XXXXXX")
    cp "$vars" "$work/vars.fd"

    if login_out=$(python3 tests/smp/boot/soak_boot.py \
        --timeout "$timeout_secs" --raw "$raw" --stamped "$stamped" -- \
        qemu-system-x86_64 \
            -machine q35 -m 512M -smp "$smp" -enable-kvm -cpu host \
            -serial stdio -display none -monitor none -no-reboot \
            -drive "if=pflash,format=raw,readonly=on,file=$code" \
            -drive "if=pflash,format=raw,file=$work/vars.fd" \
            -drive "format=raw,file=$image" \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 2>"$supervisor_log"); then
        rc=0
    else
        rc=$?
    fi
    rm -rf "$work"
    work=

    reason=
    if [ "$rc" -ne 0 ]; then
        reason="supervisor-exit-$rc"
    elif ! grep -q "smp: $smp cpus online" "$raw"; then
        reason="missing-cpu"
    elif ! grep -q 'login:' "$raw"; then
        reason="missing-login"
    elif grep -qiE 'KERNEL PANIC|panic|CPU Reset|triple fault' "$raw"; then
        reason="panic-marker"
    fi

    if [ -n "$reason" ]; then
        failed=true
        printf '%s\tfail\t0\t%s\t%s\n' "$cycle" "serials/$(basename "$raw")" "$rc" >> "$out/cycles.tsv"
        finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)
        write_meta
        write_rows
        write_hashes
        echo "s85l: FAIL cycle=$cycle/$cycles reason=$reason arch=$arch accel=$accel smp=$smp rc=$rc" >&2
        echo "s85l: serial=$raw" >&2
        [ ! -s "$supervisor_log" ] || cat "$supervisor_log" >&2
        tail -n 40 "$raw" >&2 || true
        exit 1
    fi

    login_s=${login_out#login_s=}
    printf '%s\tpass\t%s\t%s\t%s\n' "$cycle" "$login_s" "serials/$(basename "$raw")" "$rc" >> "$out/cycles.tsv"
    write_rows
    echo "s85l: PASS cycle=$cycle/$cycles login_s=$login_s"

    cycle=$((cycle + 1))
done

failed=false
finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)
write_meta
write_rows
write_hashes
echo "s85l: PASSED arch=$arch accel=$accel smp=$smp cycles=$cycles commit=$commit image_sha256=$image_sha"
echo "s85l: evidence=$out"
