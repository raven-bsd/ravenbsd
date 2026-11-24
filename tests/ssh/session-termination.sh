#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K448: SSH session termination bytes against the host OpenSSH client.
#
# A stub SSH server (paramiko, host test tooling; not OpenSSH) sends the exact termination
# sequence — exit-status 98, CHANNEL_EOF, CHANNEL_CLOSE, socket close — and negative controls omit
# each step. No QEMU, no OpenSSH edits, no other harness file changed.
#
# Requirements: python3 with paramiko, the host ssh client, tests/ssh/askpass.sh.
# Usage: sh tests/ssh/session-termination.sh
# Outer acceptance timeout: 60 s (each client attempt is bounded at 5 s).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

command -v ssh >/dev/null 2>&1 || {
    echo "session-termination: the host ssh client is required" >&2
    exit 2
}
command -v python3 >/dev/null 2>&1 || {
    echo "session-termination: python3 is required" >&2
    exit 2
}
python3 -c 'import paramiko' 2>/dev/null || {
    echo "session-termination: python3-paramiko is required (host test tooling)" >&2
    exit 2
}
[ -x tests/ssh/askpass.sh ] || {
    echo "session-termination: tests/ssh/askpass.sh is missing" >&2
    exit 2
}

exec python3 tests/ssh/session-termination.py
