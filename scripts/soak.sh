#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K228 24 h soak/freeze harness. Builds one case-specific release image with the soak sync
# fixture and the guest workload, boots it in a foreground QEMU supervised by the Python driver,
# timestamps every cycle, and stops on a workload error, boot timeout, QEMU exit or missing
# cycle-done heartbeat (stall). Logs and summary are preserved under build/soak/run-<stamp>/.
#
# `--check` is the fast script-level proof: it validates the host tools/firmware and runs the
# driver's injected-stall selftest. It never boots QEMU and never claims a 24 h pass. The owner's
# 24 h command is `timeout 90000 make soak-24h` (86400 s workload + 3600 s margin).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd -P)
cd "$root"

duration=120
interval=5
stall=
profile=release
image=build/soak/image.img
out_root=build/soak
build=0
check=0
qemu=${SOAK_QEMU:-qemu-system-x86_64}
code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}

usage() {
    cat <<'EOF'
usage: sh scripts/soak.sh [options]

  --duration S     workload duration in seconds (default 120)
  --interval S     seconds between cycles (default 5)
  --stall S        missing-cycle-done stall threshold (default max(30, interval*4))
  --profile NAME   image profile (default release)
  --image PATH     case-specific image (default build/soak/image.img)
  --build          rebuild the soak image and stage the workload-sync initfs
  --out DIR        log root (default build/soak/run-<UTC>)
  --check          validate tools/firmware and run the injected-stall selftest; no QEMU
  -h, --help       show this help

The --check path is the only one used by script-level selftests. A real run builds the image
unless it already exists; the image is built from the clean checkout at HEAD.
EOF
}

die() {
    echo "soak: $*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --duration) [ "$#" -ge 2 ] || die "--duration needs a value"; duration=$2; shift 2 ;;
        --interval) [ "$#" -ge 2 ] || die "--interval needs a value"; interval=$2; shift 2 ;;
        --stall) [ "$#" -ge 2 ] || die "--stall needs a value"; stall=$2; shift 2 ;;
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
[ "$duration" -gt 0 ] || die "duration must be positive"
[ "$interval" -gt 0 ] || die "interval must be positive"
if [ -z "$stall" ]; then
    stall=$((interval * 4))
    [ "$stall" -lt 30 ] && stall=30
fi
case $stall in ''|*[!0-9]*) die "stall must be a positive integer: $stall" ;; esac
[ "$stall" -gt 0 ] || die "stall must be positive"

driver=$root/tests/fixtures/soak/soak_driver.py

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
    echo "soak: check PASS (tools, firmware, injected stall detected)"
    exit 0
fi

check_tools

if [ "$build" -eq 1 ] || [ ! -f "$image" ]; then
    if [ -n "$(git status --porcelain -- . ':!build')" ]; then
        die "worktree is dirty; commit the soak harness before building the release image"
    fi
    mkdir -p "$out_root"
    echo "soak: building the sync fixture"
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
        CARGO_TARGET_DIR="$out_root/sync-target" \
        cargo build --manifest-path "$root/tests/fixtures/soak/sync/Cargo.toml" \
            --target x86_64-unknown-none --release
    cp "$out_root/sync-target/x86_64-unknown-none/release/raven-soak-sync" "$out_root/soak-sync"

    echo "soak: building the release initfs"
    RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile "$profile" >/dev/null
    cp build/x86_64/initfs.img "$out_root/base-initfs.img"
    echo "soak: overlaying /bin/soak-sync and /etc/soak-workload.sh"
    CARGO_TARGET_DIR="$out_root/host-target" \
        cargo run -q -p raven-b70-overlay -- \
            "$out_root/base-initfs.img" "$out_root/initfs.img" \
            "/bin/soak-sync=$out_root/soak-sync:755" \
            "/etc/soak-workload.sh=$root/tests/fixtures/soak/workload.sh:755"
    echo "soak: building the case-specific image"
    cargo xtask image --arch x86_64 --profile "$profile" \
        --initfs "$out_root/initfs.img" --output "$image"
fi

[ -f "$image" ] || die "image not found: $image"

stamp=$(date -u +%Y%m%dT%H%M%SZ)
run=$out_root/run-$stamp
mkdir -p "$run"
commit=$(git rev-parse HEAD)
echo "soak: run=$run image=$image profile=$profile duration=${duration}s interval=${interval}s stall=${stall}s commit=$commit"

if python3 "$driver" \
    --image "$image" \
    --qemu "$qemu" \
    --code "$code" \
    --vars "$vars" \
    --duration "$duration" \
    --interval "$interval" \
    --stall "$stall" \
    --out "$run" \
    --profile "$profile" \
    --commit "$commit"; then
    code_status=0
else
    code_status=$?
fi
if [ "$code_status" -eq 0 ]; then
    echo "soak: PASS summary=$run/summary.json serial=$run/serial.log"
else
    echo "soak: FAIL summary=$run/summary.json serial=$run/serial.log" >&2
fi
exit "$code_status"
