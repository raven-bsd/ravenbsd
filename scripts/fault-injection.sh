#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S71 deterministic server-death fault-injection driver.
#
# Boots the release profile, logs in, and restarts the resident data servers (netd and vfsd)
# through svcctl, then asserts the shell survives, the services come back and the restart counts
# are stable. With `--soak <seconds>` it repeats the baseline run until the budget is exhausted
# and reports the pass/fail counts, the per-run recovery time and the first failure.
#
# The release image only stages netd and vfsd as resident data servers (blockd and fsd-ufs live
# in their own test profiles), and release rc.conf pins the resident services' restart policy to
# "never", so every restart here is an explicit `svcctl start`/`svcctl restart`.

set -eu

profile=release
arch=x86_64
soak=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --profile)
            profile=$2
            shift 2
            ;;
        --arch)
            arch=$2
            shift 2
            ;;
        --soak)
            soak=$2
            shift 2
            ;;
        *)
            printf 'fault-injection: unknown option %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

if [ "$profile" != "release" ]; then
    printf 'fault-injection: only --profile release is supported today\n' >&2
    exit 2
fi

run_once() {
    cargo xtask qtest --case fault-restart --arch "$arch"
}

if [ "$soak" -le 0 ]; then
    run_once
    exit $?
fi

start=$(date +%s)
pass=0
fail=0
first_failure=
while [ $(( $(date +%s) - start )) -lt "$soak" ]; do
    run_start=$(date +%s%3N)
    if run_once >/dev/null 2>&1; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        [ -n "$first_failure" ] || first_failure=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    fi
    run_end=$(date +%s%3N)
    recovery_ms=$((run_end - run_start))
    printf 'fault-injection: run pass=%s fail=%s recovery_ms=%s\n' "$pass" "$fail" "$recovery_ms"
done
printf 'fault-injection: soak done pass=%s fail=%s first_failure=%s\n' "$pass" "$fail" "${first_failure:-none}"
[ "$fail" -eq 0 ]
