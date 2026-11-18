#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Validate a captured Pi 4 board serial log. This is an evidence checker, not a
# flasher or a serial driver: it never opens a device and never claims a board
# PASS. A complete marker set only satisfies the transcript shape; the release
# result still needs the owner's attestation and the image/log hashes.
#
# Usage:
#   scripts/board-pi4-check.sh --log LOG
#   scripts/board-pi4-check.sh --log LOG --dry-run
#   scripts/board-pi4-check.sh --help
#
# Exit codes: 0 complete, 1 incomplete, 2 refused/usage. A QEMU/virt or explicitly
# synthetic/fixture capture is refused; a missing Pi 4 board marker is refused.

set -eu

usage() {
    cat <<'EOF'
Usage: scripts/board-pi4-check.sh [options]

Validate a captured Pi 4 board serial log. The log must be produced by
scripts/pi4-serial.sh (or the same header format) and contain the release gates:
login/password/raven# prompt, `/bin/uname`, dynamic NEEDED libc.so.1, GENET link
up/DHCP, a ping reply and `init exited (0)`.

Options:
  -l, --log <path>   captured serial log to validate
      --dry-run      print the marker checklist and exit; do not read the log
  -h, --help         show this help

This script never opens a serial device and never claims a board PASS. Exit 0
means the transcript markers are complete, not that the board gate passed. A
QEMU/virt log or an explicitly synthetic/fixture log is refused with exit 2; a
capture without a Pi 4 board marker (bcm2711/raspi4b) is refused.
EOF
}

log=
dry_run=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        -l|--log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            log=$2
            shift 2
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "board-pi4-check: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$log" ]; then
    echo "board-pi4-check: --log is required" >&2
    usage >&2
    exit 2
fi

if [ "$dry_run" -eq 1 ]; then
    cat <<EOF
board-pi4-check: dry run for $log
required markers:
  capture header  # image sha256: <64 hex>, # commit: <40 hex>, # device: /dev/...
  login           login: and password: and raven#
  uname           /bin/uname output with RavenBSD and aarch64
  dynamic         NEEDED libc.so.1
  GENET           genet link up and a DHCP line
  ping            bytes from
  clean exit      init exited (0)
refused:
  any QEMU/tcg/emulator marker
  a header marking the log virt/virtual/synthetic/fixture/sample
  a missing Pi 4 board marker (bcm2711/raspi4b)
board-pi4-check: dry run only; no log read; no board result claimed
EOF
    exit 0
fi

if [ ! -f "$log" ]; then
    echo "board-pi4-check: log not found: $log" >&2
    exit 2
fi
if [ ! -s "$log" ]; then
    echo "board-pi4-check: log is empty: $log" >&2
    exit 1
fi

refuse() {
    echo "board-pi4-check: REFUSED: $1" >&2
    echo "board-pi4-check: no board result claimed" >&2
    exit 2
}

# A QEMU virt capture can carry every textual marker; refuse it before the
# marker scan instead of letting it look like a board log.
if grep -Eiq 'qemu|tcg|emulat' "$log"; then
    refuse "log contains a QEMU/tcg/emulator marker"
fi
if grep -Eiq '^#.*(virt|virtual|synthetic|fixture|sample)' "$log"; then
    refuse "capture header marks the log as virt/virtual/synthetic/fixture/sample"
fi
if grep -Eiq '(machine model|platform)[^a-z]*(virt|qemu)' "$log"; then
    refuse "log identifies the machine as virt/QEMU"
fi

# The Pi 4 DTB model or the board label must be present. A generic aarch64
# transcript is not a board result.
if ! grep -Eiq 'bcm2711|raspi4b|raspberry pi 4' "$log"; then
    refuse "no Pi 4 board marker (bcm2711/raspi4b/raspberry pi 4)"
fi

missing=0
check() {
    name=$1
    shift
    if "$@"; then
        printf '  ok      %s\n' "$name"
    else
        printf '  MISSING %s\n' "$name"
        missing=$((missing + 1))
    fi
}

check_image_hash() {
    grep -Eq '^# image[ -]sha-?256: [0-9a-f]{64}$' "$log"
}
check_commit() {
    grep -Eq '^# commit: [0-9a-f]{7,40}$' "$log"
}
check_device() {
    grep -Eq '^# device: /dev/[^ ]+' "$log"
}
check_login() {
    grep -Fq 'login:' "$log" && grep -Eiq 'password:' "$log"
}
check_prompt() {
    grep -Fq 'raven#' "$log"
}
check_uname() {
    grep -Eiq 'uname' "$log" &&
        grep -Fq 'RavenBSD' "$log" &&
        grep -Fq 'aarch64' "$log"
}
check_dynamic() {
    grep -Fq 'NEEDED' "$log" && grep -Fq 'libc.so.1' "$log"
}
check_genet_link() {
    grep -Eiq 'genet' "$log" &&
        grep -Eiq 'link[^a-z]*(is[^a-z]*)?up|carrier[^a-z]*up' "$log"
}
check_dhcp() {
    grep -Eiq 'dhcp' "$log"
}
check_ping() {
    grep -Fq 'bytes from' "$log"
}
check_clean_exit() {
    grep -Fq 'init exited (0)' "$log"
}

echo "board-pi4-check: $log"
check "capture image SHA-256" check_image_hash
check "capture commit" check_commit
check "capture device" check_device
check "login/password" check_login
check "raven# prompt" check_prompt
check "uname (RavenBSD aarch64)" check_uname
check "dynamic NEEDED libc.so.1" check_dynamic
check "GENET link up" check_genet_link
check "DHCP" check_dhcp
check "ping reply" check_ping
check "clean exit" check_clean_exit

if [ "$missing" -ne 0 ]; then
    echo "board-pi4-check: INCOMPLETE ($missing missing); no board result claimed" >&2
    exit 1
fi

cat <<'EOF'
board-pi4-check: transcript markers complete
board-pi4-check: no board PASS is claimed; attach the log, image SHA-256, commit,
image path and the owner's attestation to the release packet
EOF
exit 0
