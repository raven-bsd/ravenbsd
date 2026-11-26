#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85n 0.1.0 network and TCP gate entry point.
#
# Boots the release profile image with virtio-net and a test-only guest probe
# service, connects from the host through QEMU user-network hostfwd to the guest
# listener, and asserts the exact payload echo. Serials, host logs, summaries
# and hashes are written under tickets/evidence/artifacts/S85n/ in
# ravenbsd-prjmgmt.
#
# Usage:
#   sh tests/net/0.1.0-gate/run.sh --arch x86_64 --accel kvm
#   sh tests/net/0.1.0-gate/run.sh --arch x86_64 --accel tcg
#   sh tests/net/0.1.0-gate/run.sh --arch x86_64 --accel kvm --mutation closed-port
#
# Options:
#   --arch x86_64          image architecture (only x86_64 is a gate cell)
#   --accel kvm|tcg        accelerator (default kvm)
#   --mutation NAME        none (default), closed-port or drop-payload
#   --evidence-dir DIR     prjmgmt evidence directory (default RAVEN_EVIDENCE_DIR)
#
# Exit codes: 0 pass; 2 setup; 3 probe build; 4 overlay; 5 initfs; 6 image;
# 10 boot; 11 interface; 12 listener; 13 connect; 14 echo; 15 session; 16 panic.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)

arch=x86_64
accel=kvm
mutation=none
evidence=${RAVEN_EVIDENCE_DIR:-}

usage() {
    cat <<'EOF'
usage: sh tests/net/0.1.0-gate/run.sh --arch x86_64 --accel <kvm|tcg> [options]

  --arch x86_64          image architecture (only x86_64 is a gate cell)
  --accel kvm|tcg        accelerator (default kvm)
  --mutation NAME        none (default), closed-port or drop-payload
  --evidence-dir DIR     prjmgmt evidence directory (default RAVEN_EVIDENCE_DIR)

Evidence is written under tickets/evidence/artifacts/S85n/ in ravenbsd-prjmgmt.
Exit codes: 0 pass; 2 setup; 3 probe build; 4 overlay; 5 initfs; 6 image;
10 boot; 11 interface; 12 listener; 13 connect; 14 echo; 15 session; 16 panic.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) arch=${2:?--arch needs a value}; shift 2 ;;
        --accel) accel=${2:?--accel needs a value}; shift 2 ;;
        --mutation) mutation=${2:?--mutation needs a value}; shift 2 ;;
        --evidence-dir) evidence=${2:?--evidence-dir needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "run.sh: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$arch" in
    x86_64) ;;
    *) echo "run.sh: unsupported --arch $arch (x86_64 is the gate cell)" >&2; exit 2 ;;
esac
case "$accel" in
    kvm|tcg) ;;
    *) echo "run.sh: unsupported --accel $accel (kvm or tcg)" >&2; exit 2 ;;
esac
case "$mutation" in
    none|closed-port|drop-payload) ;;
    *) echo "run.sh: unsupported --mutation $mutation" >&2; exit 2 ;;
esac

command -v python3 >/dev/null 2>&1 || { echo "run.sh: python3 is required" >&2; exit 2; }
command -v cargo >/dev/null 2>&1 || { echo "run.sh: cargo is required" >&2; exit 2; }
command -v qemu-system-x86_64 >/dev/null 2>&1 || {
    echo "run.sh: qemu-system-x86_64 is required" >&2
    exit 2
}

if [ -z "$evidence" ]; then
    probe=$root
    while [ "$probe" != "/" ]; do
        if [ -d "$probe/ravenbsd-prjmgmt/tickets" ]; then
            evidence=$probe/ravenbsd-prjmgmt/tickets/evidence/artifacts/S85n
            break
        fi
        probe=$(dirname "$probe")
    done
fi
if [ -z "$evidence" ]; then
    echo "run.sh: cannot locate ravenbsd-prjmgmt; set RAVEN_EVIDENCE_DIR" >&2
    exit 2
fi
mkdir -p "$evidence"

exec python3 "$here/gate.py" \
    --arch "$arch" --accel "$accel" --mutation "$mutation" --evidence-dir "$evidence"
