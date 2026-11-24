#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K285 TCP soak harness. Builds the tcp-soak profile image (the release wiring plus nc
# and the guest TCP workload), boots it in a foreground QEMU supervised by
# scripts/tcp-soak-driver.py, and fails on a workload error, a boot timeout, a QEMU
# exit or a missing cycle-done heartbeat (stall). A stall is a failure, never a retry:
# the driver dumps the last socket-state and serial evidence and exits non-zero. Logs
# and the summary are preserved under build/tcp-soak/run-<stamp>/.
#
# `--check` is the fast script-level proof: it validates the host tools/firmware and
# runs the driver's injected-stall selftest. It never boots QEMU and never claims a
# 24 h pass. The owner's 24 h command is `timeout 90000 make tcp-soak-24h`
# (86400 s workload + 3600 s margin).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd -P)
cd "$root"

duration=600
interval=10
stall=
port=7000
profile=tcp-soak
image=build/tcp-soak/image.img
out_root=build/tcp-soak
build=0
check=0
qemu=${TCP_SOAK_QEMU:-qemu-system-x86_64}
code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}

usage() {
    cat <<'EOF'
usage: sh scripts/tcp-soak.sh [options]

  --duration S     workload duration in seconds (default 600, the bounded smoke)
  --interval S     seconds between cycles (default 10)
  --stall S        missing-cycle-done stall threshold (default max(30, interval*4))
  --port P         guest echo port (default 7000)
  --profile NAME   image profile (default tcp-soak)
  --image PATH     case-specific image (default build/tcp-soak/image.img)
  --build          rebuild the tcp-soak image
  --out DIR        log root (default build/tcp-soak)
  --check          validate tools/firmware and run the injected-stall selftest; no QEMU
  -h, --help       show this help

The --check path is the only one used by script-level selftests. A real run builds the
image unless it already exists; the image is built from the clean checkout at HEAD.
EOF
}

die() {
    echo "tcp-soak: $*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --duration) [ "$#" -ge 2 ] || die "--duration needs a value"; duration=$2; shift 2 ;;
        --interval) [ "$#" -ge 2 ] || die "--interval needs a value"; interval=$2; shift 2 ;;
        --stall) [ "$#" -ge 2 ] || die "--stall needs a value"; stall=$2; shift 2 ;;
        --port) [ "$#" -ge 2 ] || die "--port needs a value"; port=$2; shift 2 ;;
        --profile) [ "$#" -ge 2 ] || die "--profile needs a value"; profile=$2; shift 2 ;;
        --image) [ "$#" -ge 2 ] || die "--image needs a value"; image=$2; shift 2 ;;
        --build) build=1; shift ;;
        --out) [ "$#" -ge 2 ] || die "--out needs a value"; out_root=$2; shift 2 ;;
        --check) check=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
done

case $duration in ''|*[!0-9]*) die "duration must be a positive integer: $duration" ;; esac
case $interval in ''|*[!0-9]*) die "interval must be a positive integer: $interval" ;; esac
case $port in ''|*[!0-9]*) die "port must be a positive integer: $port" ;; esac
[ "$duration" -gt 0 ] || die "duration must be positive"
[ "$interval" -gt 0 ] || die "interval must be positive"
[ "$port" -ge 1 ] && [ "$port" -le 65535 ] || die "port must be 1..65535"
if [ -z "$stall" ]; then
    stall=$((interval * 4))
    [ "$stall" -lt 30 ] && stall=30
fi
case $stall in ''|*[!0-9]*) die "stall must be a positive integer: $stall" ;; esac
[ "$stall" -gt 0 ] || die "stall must be positive"

driver=$root/scripts/tcp-soak-driver.py

check_tools() {
    command -v python3 >/dev/null 2>&1 || die "python3 not found"
    command -v "$qemu" >/dev/null 2>&1 || die "$qemu not found"
    command -v timeout >/dev/null 2>&1 || die "timeout not found"
    [ -f "$code" ] || die "OVMF code not found: $code"
    [ -f "$vars" ] || die "OVMF vars not found: $vars"
    python3 "$driver" --selftest || die "injected-stall selftest failed"
}

if [ "$check" -eq 1 ]; then
    check_tools
    echo "tcp-soak: check PASS (tools, firmware, injected stall detected)"
    exit 0
fi

check_tools

if [ "$build" -eq 1 ] || [ ! -f "$image" ]; then
    if [ -n "$(git status --porcelain -- . ':!build')" ]; then
        die "worktree is dirty; commit the tcp-soak harness before building the image"
    fi
    mkdir -p "$out_root"
    echo "tcp-soak: building the $profile image (this is the first-run cost)"
    RAVEN_USER_RELEASE=1 cargo xtask image --arch x86_64 --profile "$profile" --output "$image"
fi

[ -f "$image" ] || die "image not found: $image"

stamp=$(date -u +%Y%m%dT%H%M%SZ)
run=$out_root/run-$stamp
mkdir -p "$run"
commit=$(git rev-parse HEAD)
echo "tcp-soak: run=$run image=$image profile=$profile duration=${duration}s interval=${interval}s stall=${stall}s port=${port} commit=$commit"

if python3 "$driver" \
    --image "$image" \
    --qemu "$qemu" \
    --code "$code" \
    --vars "$vars" \
    --duration "$duration" \
    --interval "$interval" \
    --stall "$stall" \
    --port "$port" \
    --out "$run" \
    --profile "$profile" \
    --commit "$commit"; then
    code_status=0
else
    code_status=$?
fi
if [ "$code_status" -eq 0 ]; then
    echo "tcp-soak: PASS summary=$run/summary.json serial=$run/serial.log socket-state=$run/socket-state.log"
else
    echo "tcp-soak: FAIL summary=$run/summary.json evidence=$run/stall-evidence.txt" >&2
fi
exit "$code_status"
