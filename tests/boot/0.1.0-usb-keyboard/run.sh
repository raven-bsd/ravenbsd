#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85p 0.1.0 USB keyboard console login gate.
#
# Builds the release profile image, boots it under QEMU with `qemu-xhci` and
# `usb-kbd`, and types the brush login dialogue as QMP input events routed to
# the USB keyboard display. The routed display excludes the PS/2 keyboard and
# the serial console is output-only, so a pass cannot come from PS/2 or serial
# input. Every serial, summary and hash is written under
# tickets/evidence/artifacts/S85p/ in ravenbsd-prjmgmt.
#
# Usage:
#   sh tests/boot/0.1.0-usb-keyboard/run.sh --arch x86_64 --accel kvm
#   sh tests/boot/0.1.0-usb-keyboard/run.sh --arch x86_64 --accel tcg
#   sh tests/boot/0.1.0-usb-keyboard/run.sh --mutation wrong-password
#   sh tests/boot/0.1.0-usb-keyboard/run.sh --mutation wrong-password
#
# Exit codes: 0 pass; 2 setup; 3 image; 10 boot; 11 input; 12 login;
# 13 marker; 14 panic.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
here_logical=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd -P)

arch=x86_64
accel=
mutation=none
evidence=
force_build=0

usage() {
    cat <<'EOF'
usage:
  run.sh --arch x86_64 --accel kvm|tcg [--mutation none|wrong-password]
         [--evidence-dir DIR] [--force-build]

  --accel        QEMU accelerator; kvm requires an accessible /dev/kvm
  --arch         image architecture (only x86_64 is the gate cell)
  --mutation     none (default) or wrong-password
  --evidence-dir evidence directory (default tickets/evidence/artifacts/S85p)
  --force-build  rebuild the release image even when the commit stamp matches

Exit codes: 0 pass; 2 setup; 3 image; 10 boot; 11 input; 12 login;
13 marker; 14 panic.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) arch=${2:?--arch needs a value}; shift 2 ;;
        --accel) accel=${2:?--accel needs a value}; shift 2 ;;
        --mutation) mutation=${2:?--mutation needs a value}; shift 2 ;;
        --evidence-dir) evidence=${2:?--evidence-dir needs a value}; shift 2 ;;
        --force-build) force_build=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "run.sh: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [ -z "$accel" ]; then
    echo "run.sh: --accel is required" >&2
    usage >&2
    exit 2
fi
case "$arch" in
    x86_64) ;;
    *) echo "run.sh: unsupported --arch $arch (x86_64 is the gate cell)" >&2; exit 2 ;;
esac
case "$accel" in
    kvm|tcg) ;;
    *) echo "run.sh: unsupported --accel $accel (kvm or tcg)" >&2; exit 2 ;;
esac
case "$mutation" in
    none|wrong-password) ;;
    *) echo "run.sh: unsupported --mutation $mutation" >&2; exit 2 ;;
esac

command -v python3 >/dev/null 2>&1 || { echo "run.sh: python3 is required" >&2; exit 2; }
command -v cargo >/dev/null 2>&1 || { echo "run.sh: cargo is required" >&2; exit 2; }
command -v qemu-system-x86_64 >/dev/null 2>&1 || {
    echo "run.sh: qemu-system-x86_64 is required" >&2
    exit 2
}

find_prjmgmt() {
    base=$1
    while [ "$base" != "/" ]; do
        if [ -d "$base/ravenbsd-prjmgmt/tickets" ]; then
            printf '%s\n' "$base/ravenbsd-prjmgmt"
            return 0
        fi
        base=$(dirname "$base")
    done
    return 1
}

if [ -z "$evidence" ]; then
    if [ -n "${RAVEN_EVIDENCE_DIR:-}" ]; then
        evidence=$RAVEN_EVIDENCE_DIR
    else
        prjmgmt=$(find_prjmgmt "$here_logical" || find_prjmgmt "$root") || prjmgmt=
        if [ -z "$prjmgmt" ] && [ -f "$root/DIRECTIVE.md" ]; then
            report=$(awk '/^Report file: /{print $3; exit}' "$root/DIRECTIVE.md")
            case "$report" in
                */tickets/*) prjmgmt=${report%%/tickets/*} ;;
            esac
        fi
        [ -n "$prjmgmt" ] || {
            echo "run.sh: cannot locate ravenbsd-prjmgmt; set RAVEN_EVIDENCE_DIR" >&2
            exit 2
        }
        evidence=$prjmgmt/tickets/evidence/artifacts/S85p
    fi
fi
mkdir -p "$evidence"

if [ "$force_build" -eq 1 ]; then
    exec python3 "$here/gate.py" \
        --arch "$arch" --accel "$accel" --mutation "$mutation" \
        --evidence-dir "$evidence" --force-build
fi
exec python3 "$here/gate.py" \
    --arch "$arch" --accel "$accel" --mutation "$mutation" \
    --evidence-dir "$evidence"
