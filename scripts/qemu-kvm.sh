#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Reproducible KVM SMP boot for the x86_64 release image (S83).
#
# Boots the release image with -smp 1, 2 and 4 under KVM and requires
# `smp: N cpus online` plus the release `login:` prompt for each. This is a
# developer/CI aid; the TCG qtests remain the release path.
#
# The release image build requires a clean worktree. Build it once on the
# commit under test, then run with --no-build:
#
#   cargo xtask image --arch x86_64 --profile release
#   sh scripts/qemu-kvm.sh --no-build
#
# Usage: sh scripts/qemu-kvm.sh [--no-build] [--image PATH] [--timeout SECONDS]

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

image=build/x86_64/ravenbsd-x86_64.img
profile=release
no_build=0
timeout_secs=180
smp_list=1,2,4
iterations=1

usage() {
    cat <<'EOF'
usage: sh scripts/qemu-kvm.sh [options]

  --no-build         use the existing image (do not run cargo xtask image)
  --image PATH       release image (default: build/x86_64/ravenbsd-x86_64.img)
  --timeout SECONDS  per-boot timeout (default: 180)
  --smp LIST         comma-separated CPU counts (default: 1,2,4)
  --iterations N     boot the selected set N times (default: 1)
  -h, --help         show this help

Boots -smp 1, 2 and 4 with -enable-kvm -cpu host and requires the SMP online
line and the release login prompt. The release image build needs a clean tree.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --no-build)
            no_build=1
            shift
            ;;
        --image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            image=$2
            shift 2
            ;;
        --timeout)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            timeout_secs=$2
            shift 2
            ;;
        --smp)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            smp_list=$2
            shift 2
            ;;
        --iterations)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            iterations=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "qemu-kvm: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ "$no_build" -eq 0 ]; then
    cargo xtask image --arch x86_64 --profile "$profile"
fi

if [ ! -f "$image" ]; then
    echo "qemu-kvm: image not found: $image (build it or drop --no-build)" >&2
    exit 2
fi
if [ ! -w /dev/kvm ]; then
    echo "qemu-kvm: /dev/kvm is not writable" >&2
    exit 2
fi

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
if [ ! -f "$code" ] || [ ! -f "$vars" ]; then
    echo "qemu-kvm: OVMF firmware not found" >&2
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/qemu-kvm.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

smp_set=$(printf '%s' "$smp_list" | tr ',' ' ')
iteration=1
while [ "$iteration" -le "$iterations" ]; do
    for smp in $smp_set; do
        log=$work/smp-$iteration-$smp.log
        cp "$vars" "$work/vars-$iteration-$smp.fd"
        echo "qemu-kvm: boot $iteration/$iterations -smp $smp"
        set +e
        timeout "$timeout_secs" qemu-system-x86_64 \
            -machine q35 -m 512M -smp "$smp" -enable-kvm -cpu host \
            -serial stdio -display none -monitor none -no-reboot \
            -drive "if=pflash,format=raw,readonly=on,file=$code" \
            -drive "if=pflash,format=raw,file=$work/vars-$iteration-$smp.fd" \
            -drive "format=raw,file=$image" \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            >"$log" 2>&1
        status=$?
        set -e
        if ! grep -q "smp: $smp cpus online" "$log"; then
            echo "qemu-kvm: FAIL iteration=$iteration -smp $smp: no 'smp: $smp cpus online' (exit $status)" >&2
            tail -n 30 "$log" >&2
            exit 1
        fi
        if ! grep -q 'login:' "$log"; then
            echo "qemu-kvm: FAIL iteration=$iteration -smp $smp: no login prompt (exit $status)" >&2
            tail -n 30 "$log" >&2
            exit 1
        fi
        echo "qemu-kvm: PASS iteration=$iteration -smp $smp"
    done
    iteration=$((iteration + 1))
done
echo "qemu-kvm: PASSED -smp $smp_list iterations=$iterations"
