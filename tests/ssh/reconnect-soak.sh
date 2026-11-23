#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K449: SSH reconnect soak over the K194 tap. Fresh command sessions (and optionally TTY
# sessions) against the release gate image; every session records exit, wall, exact marker bytes
# and the client-observed server lines. No retries; a failure is reported with its session number.
#
# Usage: sh tests/ssh/reconnect-soak.sh [--sessions N] [--batch N] [--tty-sessions N]
#                                       [--session-timeout S] [--keep-going] [--net tap|user]
#                                       [--accel tcg|kvm]
# The 100/100 gate: sh tests/ssh/reconnect-soak.sh --sessions 100 --tty-sessions 10
# --net user (QEMU host forward, no tap) is for a host where the shared K194 tap is unavailable;
# the release gate uses the tap.
# Outer acceptance timeout: 3600 s for 100 sessions; a bounded measurement uses a smaller
# --sessions and its own cap.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

command -v ssh >/dev/null 2>&1 || {
    echo "reconnect-soak: the host ssh client is required" >&2
    exit 2
}
command -v python3 >/dev/null 2>&1 || {
    echo "reconnect-soak: python3 is required" >&2
    exit 2
}
command -v qemu-system-x86_64 >/dev/null 2>&1 || {
    echo "reconnect-soak: qemu-system-x86_64 is required" >&2
    exit 2
}
needs_root=1
case " $* " in
    *" --net user "*) needs_root=0 ;;
esac
if [ "$needs_root" -eq 1 ]; then
    sudo -n true 2>/dev/null || {
        echo "reconnect-soak: root required for the K194 tap helper (passwordless sudo unavailable)" >&2
        exit 2
    }
fi
[ -x tests/ssh/askpass.sh ] || {
    echo "reconnect-soak: tests/ssh/askpass.sh is missing" >&2
    exit 2
}

exec python3 tests/ssh/reconnect-soak.py "$@"
