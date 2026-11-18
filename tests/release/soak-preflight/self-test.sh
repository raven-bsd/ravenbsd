#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Synthetic self-tests for scripts/soak-preflight.sh. These cases fake the host probes;
# they never start QEMU, cargo or the 24 h soak.
#
# Usage: sh tests/release/soak-preflight/self-test.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
preflight=$root/scripts/soak-preflight.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/soak-preflight-selftest.XXXXXX")
trap 'rm -rf "$work"' EXIT

: > "$work/empty"
: > "$work/qemu"
: > "$work/build"
failures=0

run_preflight() {
    SOAK_PREFLIGHT_QEMU_LIST=$work/empty \
    SOAK_PREFLIGHT_BUILD_LIST=$work/empty \
    SOAK_PREFLIGHT_DISK_AVAIL_KB=99999999 \
    SOAK_PREFLIGHT_MEM_AVAIL_KB=99999999 \
    SOAK_PREFLIGHT_LOAD1=0 \
    SOAK_PREFLIGHT_NPROC=8 \
    SOAK_PREFLIGHT_LOG_DIR=$work/logs \
    SOAK_PREFLIGHT_GIT_DIRTY=0 \
    SOAK_PREFLIGHT_FAULT_SELF_TEST="printf 'fault-injection self-test: PASS\n'" \
    env "$@" sh "$preflight" --check
}

run_case() {
    name=$1
    expected=$2
    pattern=$3
    shift 3
    set +e
    out=$(run_preflight "$@" 2>&1)
    code=$?
    set -e
    if [ "$code" -ne "$expected" ]; then
        printf 'FAIL %s: exit %s, want %s\n%s\n' "$name" "$code" "$expected" "$out" >&2
        failures=$((failures + 1))
        return
    fi
    if ! printf '%s\n' "$out" | grep -q "$pattern"; then
        printf 'FAIL %s: missing %s\n%s\n' "$name" "$pattern" "$out" >&2
        failures=$((failures + 1))
        return
    fi
    printf 'ok   %s (exit %s)\n' "$name" "$code"
}

# 1. Idle host: all checks pass and the exact clean-run command is printed.
run_case idle 0 'READY'
run_case idle-command 0 'command: timeout 90000 sh scripts/fault-injection.sh'

# 2. Busy QEMU.
printf '123 qemu-system-x86_64 -machine q35\n' > "$work/qemu"
run_case busy-qemu 1 'busy: QEMU' env SOAK_PREFLIGHT_QEMU_LIST="$work/qemu"
: > "$work/qemu"

# 3. Concurrent build/qtest.
printf '456 cargo test -p raven-libc\n' > "$work/build"
run_case busy-build 1 'busy: build/qtest/soak' env SOAK_PREFLIGHT_BUILD_LIST="$work/build"
: > "$work/build"

# 4. Low disk and low memory.
run_case low-disk 1 'low disk:' env SOAK_PREFLIGHT_DISK_AVAIL_KB=1024
run_case low-memory 1 'low memory:' env SOAK_PREFLIGHT_MEM_AVAIL_KB=1024

# 5. The patched fault-injection self-test fails.
run_case bad-fault-self-test 1 'fault-injection self-test failed' \
    env SOAK_PREFLIGHT_FAULT_SELF_TEST=false

# 6. Dirty worktree.
run_case dirty-worktree 1 'worktree is dirty' env SOAK_PREFLIGHT_GIT_DIRTY=1

# 7. A stale log/sidecar path is refused.
mkdir -p "$work/stale"
printf 'old\n' > "$work/stale/soak-20260101T000000Z.log"
run_case stale-log 1 'log/sidecar path already exists' \
    env SOAK_PREFLIGHT_LOG_DIR="$work/stale" SOAK_PREFLIGHT_STAMP=20260101T000000Z

# 8. --help is usable and does not probe the host.
set +e
out=$(sh "$preflight" --help 2>&1)
code=$?
set -e
if [ "$code" -eq 0 ] && printf '%s\n' "$out" | grep -q 'usage: sh scripts/soak-preflight.sh --check'; then
    printf 'ok   help (exit 0)\n'
else
    printf 'FAIL help: exit %s\n%s\n' "$code" "$out" >&2
    failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
    printf 'soak-preflight self-test: FAILED (%s)\n' "$failures" >&2
    exit 1
fi
printf 'soak-preflight self-test: PASSED (synthetic checks only; no QEMU, no soak)\n'
