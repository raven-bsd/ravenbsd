#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K300 TCP soak evidence runner entry point. Wraps the K285 soak
# (scripts/tcp-soak.sh + scripts/tcp-soak-driver.py) with a host-side stall
# monitor and post-run artifact-hash evidence collection. A stall is a
# failure, never a retry: on stall the runner preserves
# runner-stall-evidence.txt (stream tails plus a sha256 manifest of whatever
# exists) and exits non-zero.
#
# The owner's 24 h command is owner-only and is NOT claimed by the K300
# ticket:
#
#   timeout 90000 sh scripts/tcp-soak-evidence.sh --run --duration 86400 \
#       --interval 30 --stall 300
#
# The ticket's bounded smoke proof goes through the same runner code path:
#
#   timeout 300 sh scripts/tcp-soak-evidence.sh --run --duration 120 \
#       --interval 10 --stall 300 --out build/tcp-soak-evidence
#
# `--check` is the fast script-level proof: it validates python3 and runs
# both selftests (the K300 evidence collector and the K285 driver). It never
# boots QEMU and never claims a 24 h pass.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd -P)
cd "$root"

mode=""
duration=600
interval=10
stall=300
port=
out_root=build/tcp-soak
image=
target=

usage() {
    cat <<'EOF'
usage: sh scripts/tcp-soak-evidence.sh --check
       sh scripts/tcp-soak-evidence.sh --run [--duration S] [--interval S]
                                       [--stall S] [--port P] [--out DIR]
                                       [--image PATH]
       sh scripts/tcp-soak-evidence.sh --collect RUN_DIR [--image PATH]
       sh scripts/tcp-soak-evidence.sh --verify EVIDENCE_JSON
  --check             validate python3 and run both selftests; no QEMU
  --run ...           run the K285 soak under the host-side stall monitor,
                      then collect artifact-hash evidence for the run
  --collect RUN_DIR   collect evidence for an already finished run directory
  --verify FILE       re-hash the artifacts recorded in EVIDENCE_JSON;
                      exit non-zero on any missing file or hash mismatch
  --duration S        workload duration in seconds (default 600)
  --interval S        seconds between cycles (default 10)
  --stall S           runner-level no-progress threshold in seconds
                      (default 300); independent of the driver's guest-level
                      stall threshold
  --port P            guest echo port (default 7000)
  --out DIR           log root (default build/tcp-soak)
  --image PATH        case-specific image (default build/tcp-soak/image.img)
EOF
}

die() {
    echo "tcp-soak-evidence: $*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) mode=check; shift ;;
        --run) mode=run; shift ;;
        --collect) [ "$#" -ge 2 ] || die "--collect needs RUN_DIR"; mode=collect; target=$2; shift 2 ;;
        --verify) [ "$#" -ge 2 ] || die "--verify needs EVIDENCE_JSON"; mode=verify; target=$2; shift 2 ;;
        --duration) [ "$#" -ge 2 ] || die "--duration needs a value"; duration=$2; shift 2 ;;
        --interval) [ "$#" -ge 2 ] || die "--interval needs a value"; interval=$2; shift 2 ;;
        --stall) [ "$#" -ge 2 ] || die "--stall needs a value"; stall=$2; shift 2 ;;
        --port) [ "$#" -ge 2 ] || die "--port needs a value"; port=$2; shift 2 ;;
        --out) [ "$#" -ge 2 ] || die "--out needs a value"; out_root=$2; shift 2 ;;
        --image) [ "$#" -ge 2 ] || die "--image needs a value"; image=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
done

case $duration in ''|*[!0-9]*) die "duration must be a positive integer: $duration" ;; esac
case $interval in ''|*[!0-9]*) die "interval must be a positive integer: $interval" ;; esac
[ "$duration" -gt 0 ] || die "duration must be positive"
[ "$interval" -gt 0 ] || die "interval must be positive"

case $mode in
    check)
        command -v python3 >/dev/null 2>&1 || die "python3 not found"
        [ -f scripts/tcp-soak.sh ] || die "scripts/tcp-soak.sh (K285) not found"
        [ -f scripts/tcp-soak-driver.py ] || die "scripts/tcp-soak-driver.py (K285) not found"
        python3 scripts/tcp-soak-evidence.py --selftest || die "evidence collector selftest failed"
        python3 scripts/tcp-soak-driver.py --selftest || die "K285 driver selftest failed"
        echo "tcp-soak-evidence: check PASS (python3, collector selftest, K285 driver selftest)"
        ;;
    run)
        args="--run --duration $duration --interval $interval --stall $stall --out $out_root"
        [ -n "$port" ] && args="$args --port $port"
        [ -n "$image" ] && args="$args --image $image"
        # shellcheck disable=SC2086
        exec python3 scripts/tcp-soak-evidence.py $args
        ;;
    collect)
        args="--collect $target"
        [ -n "$image" ] && args="$args --image $image"
        # shellcheck disable=SC2086
        exec python3 scripts/tcp-soak-evidence.py $args
        ;;
    verify)
        exec python3 scripts/tcp-soak-evidence.py --verify "$target"
        ;;
    *)
        usage >&2
        die "choose --check, --run, --collect RUN_DIR or --verify EVIDENCE_JSON"
        ;;
esac
