#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85k 0.1.0 SSH login gate entry point.
#
# Boots the release profile image, proves the staged sshd is listening, logs in
# with the host OpenSSH client, runs a marker command and a PTY command, and
# asserts a clean session close. Evidence (serial, client logs, summary and
# hashes) is written under tickets/evidence/artifacts/S85k/ in ravenbsd-prjmgmt.
#
# Usage:
#   sh tests/ssh/0.1.0-login-gate/run.sh --arch x86_64 --accel kvm
#   sh tests/ssh/0.1.0-login-gate/run.sh --arch x86_64 --accel tcg
#   sh tests/ssh/0.1.0-login-gate/run.sh --arch aarch64 --accel tcg
#
# Options:
#   --arch x86_64|aarch64     image architecture (default x86_64)
#   --accel kvm|tcg           accelerator (default kvm; aarch64 is tcg only)
#   --image PATH              reuse an image instead of building the release image
#   --evidence-dir DIR        prjmgmt evidence directory (default RAVEN_EVIDENCE_DIR)
#   --password PW             host login password (default raven; mutation knob)
#
# Exit codes: 0 pass; 2 setup; 3 image build; 10 boot; 12 missing sshd;
# 13 incomplete sshd; 14 sshd not listening; 15 handshake; 16 authentication;
# 17 PTY; 18 session/command.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)

arch=x86_64
accel=kvm
image=
evidence=${RAVEN_EVIDENCE_DIR:-}
password=${RAVEN_SSH_PASSWORD:-raven}

usage() {
    cat <<'EOF'
usage: sh tests/ssh/0.1.0-login-gate/run.sh --arch <x86_64|aarch64> --accel <kvm|tcg> [options]

  --arch x86_64|aarch64     image architecture (default x86_64)
  --accel kvm|tcg           accelerator (default kvm; aarch64 is tcg only)
  --image PATH              reuse an image instead of building the release image
  --evidence-dir DIR        prjmgmt evidence directory (default RAVEN_EVIDENCE_DIR)
  --password PW             host login password (default raven; mutation knob)

Evidence is written under tickets/evidence/artifacts/S85k/ in ravenbsd-prjmgmt.
Exit codes: 0 pass; 2 setup; 3 image build; 10 boot; 12 missing sshd;
13 incomplete sshd; 14 sshd not listening; 15 handshake; 16 authentication;
17 PTY; 18 session/command.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) arch=${2:?--arch needs a value}; shift 2 ;;
        --accel) accel=${2:?--accel needs a value}; shift 2 ;;
        --image) image=${2:?--image needs a value}; shift 2 ;;
        --evidence-dir) evidence=${2:?--evidence-dir needs a value}; shift 2 ;;
        --password) password=${2:?--password needs a value}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "run.sh: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$arch" in
    x86_64|aarch64) ;;
    *) echo "run.sh: unsupported --arch $arch (x86_64 or aarch64)" >&2; exit 2 ;;
esac
case "$accel" in
    kvm|tcg) ;;
    *) echo "run.sh: unsupported --accel $accel (kvm or tcg)" >&2; exit 2 ;;
esac

command -v python3 >/dev/null 2>&1 || { echo "run.sh: python3 is required" >&2; exit 2; }
command -v ssh >/dev/null 2>&1 || { echo "run.sh: the host OpenSSH client is required" >&2; exit 2; }
python3 -c 'import pexpect' >/dev/null 2>&1 || {
    echo "run.sh: the python3 pexpect module is required" >&2
    exit 2
}
case "$arch" in
    x86_64) qemu=qemu-system-x86_64 ;;
    aarch64) qemu=qemu-system-aarch64 ;;
esac
command -v "$qemu" >/dev/null 2>&1 || { echo "run.sh: $qemu is required" >&2; exit 2; }
if [ -z "$image" ]; then
    command -v cargo >/dev/null 2>&1 || { echo "run.sh: cargo is required for the image build" >&2; exit 2; }
fi

if [ -z "$evidence" ]; then
    probe=$root
    while [ "$probe" != "/" ]; do
        if [ -d "$probe/ravenbsd-prjmgmt/tickets" ]; then
            evidence=$probe/ravenbsd-prjmgmt/tickets/evidence/artifacts/S85k
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

set -- --arch "$arch" --accel "$accel" --evidence-dir "$evidence" --password "$password"
if [ -n "$image" ]; then
    set -- "$@" --image "$image"
fi
exec python3 "$here/gate.py" "$@"
