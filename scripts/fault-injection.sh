#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S71 deterministic server-death fault-injection driver.
#
# With no selector it runs the cross-architecture fault cases:
#
#   fault-restart    after-reply, during-restart and idle STOP/START on the `frt` worker
#   fault-reconnect  in-flight request death: EIO/EPIPE, generation change, bounded reopen
#
# `--profile release` selects the x86_64 release-image matrix (netd and vfsd through svcctl).
# `--case <name>` runs one case. With `--soak <seconds>` the selected set repeats until the
# budget is exhausted and the driver reports the pass/fail counts, the per-run wall time and the
# first failure.

set -eu

profile=
arch=x86_64
soak=0
selected_case=

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
        --case)
            selected_case=$2
            shift 2
            ;;
        *)
            printf 'fault-injection: unknown option %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

if [ -n "$selected_case" ]; then
    cases=$selected_case
elif [ "$profile" = "release" ]; then
    cases=fault-restart-release
elif [ -z "$profile" ]; then
    cases="fault-restart fault-reconnect"
else
    printf 'fault-injection: only --profile release or the default cross-arch set is supported\n' >&2
    exit 2
fi

run_once() {
    for case_name in $cases; do
        cargo xtask qtest --case "$case_name" --arch "$arch" || return $?
    done
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
