#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Check a captured release-login transcript for the REL2b gate milestones.
# Usage: check-transcript.sh [build/qtest/release-login.log]

set -eu
log=${1:-build/qtest/release-login.log}

if [ ! -f "$log" ]; then
    echo "release transcript: missing $log" >&2
    exit 1
fi

check() {
    pattern=$1
    label=$2
    if ! grep -qE "$pattern" "$log"; then
        echo "release transcript: missing $label" >&2
        exit 1
    fi
}

check '^loader: RavenBSD loader 0\.1' "loader banner"
check '^RavenBSD kernel 0\.1' "kernel banner"
if ! grep -qiE 'login: *$' "$log"; then
    echo "release transcript: missing login prompt" >&2
    exit 1
fi
if ! grep -qiE 'password: *$' "$log"; then
    echo "release transcript: missing password prompt" >&2
    exit 1
fi
check '^root@.*# ?$' "brush root prompt"
check '^cat$' "ls /bin: cat"
check '^ls$' "ls /bin: ls"
check '^uname$' "ls /bin: uname"
check '^ping: 10\.0\.2\.2 seq=1 rtt=[0-9]+ms$' "ping reply"
check '^ping: [1-9][0-9]* packets transmitted, [1-9][0-9]* received, 0% packet loss$' "ping summary"
check '^init exited \(0\)$' "clean init exit"

echo "release transcript: ok ($log)"
