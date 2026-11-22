#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K435: run one test per SSH kernel-contract row.
#
# Row 1 (poll/select level readiness), row 2 (partial read/EAGAIN), row 3 (partial write/EAGAIN)
# and the dup-readiness half of row 5 run in the ssh-contract fixture. Row 4 runs libc-forkpty
# (the PTY pair, grant/unlock, ptsname, termios path), and the fork/exec half of row 5 runs
# socketpair-exec and socket-write-exec. socket-read-efault covers the valid COW buffer that must
# not return EFAULT.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

run_case() {
    name=$1
    echo "=== ssh-contract: $name"
    timeout 300 cargo xtask qtest --case "$name" --arch x86_64
}

echo "=== ssh-contract: local fixture"
timeout 300 cargo xtask qtest --case tests/smp/ssh-contract/ssh-contract.toml --arch x86_64

run_case socket-read-efault
run_case socketpair-exec
run_case socket-write-exec
run_case libc-forkpty

echo "ssh-contract: all rows passed"
