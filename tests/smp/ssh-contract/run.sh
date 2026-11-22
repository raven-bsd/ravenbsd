#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K435: run the guest SSH kernel-contract conformance suite and the supporting controls.
# The primary suite prints CONTRACT K1..K6 PASS|FAIL and exits non-zero on the first red row.
# socket-read-efault, socketpair-exec, socket-write-exec and libc-forkpty are the donor-path
# controls for the same rows.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

run_case() {
    name=$1
    echo "=== ssh-contract: $name"
    timeout 300 cargo xtask qtest --case "$name" --arch x86_64
}

echo "=== ssh-contract: guest conformance suite"
pkill -f "qemu-system.*ssh-contract.img" 2>/dev/null || true
pkill -f "ssh-contract/host_accept.py" 2>/dev/null || true
rm -f build/qtest/ssh-contract.img build/qtest/ssh-contract.vars.fd
timeout 300 cargo xtask qtest --case tests/smp/ssh-contract/ssh-contract.toml --arch x86_64

if [ -f build/fixtures/ssh-contract/host.result ]; then
    if [ "$(cat build/fixtures/ssh-contract/host.result)" != "ok" ]; then
        echo "ssh-contract: K6 host peer failed" >&2
        exit 1
    fi
fi

run_case socket-read-efault
run_case socketpair-exec
run_case socket-write-exec
run_case libc-forkpty

echo "ssh-contract: all rows passed"
