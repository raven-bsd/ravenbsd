#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Committed interface sweep for both gate architectures. Runs the full present-interface
# declaration sweep from the posix-conform tool; documented extension rows are reported as
# EXTENSION, any other failure is a required-declaration regression. Pass --strict to treat the
# extension list as failures too (for P1f7 to flip once the waiver list is gone).
#
# `assert` is a function-like macro in POSIX and ISO C, so the sweep's generic
# `(void *)&symbol` probe cannot name it. This script verifies the macro with a real `assert(1)`
# use under the same feature macros and reports the row as MACRO; a failing macro probe or any
# other required failure still fails the sweep.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

macro_probe() {
    printf '#include <assert.h>\nint main(void) { assert(1); return 0; }\n' \
        >"$tmp/assert-probe.c"
    cc -I ./libc/ravenlibc/include -fsyntax-only -std=c17 \
        -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 "$tmp/assert-probe.c"
}

run_sweep() {
    arch=$1
    shift
    log="$tmp/$arch.log"
    set +e
    cargo run -q -p posix-conform -- sweep --arch "$arch" "$@" >"$log" 2>&1
    status=$?
    set -e

    required=$(grep -E '^FAIL ' "$log" | grep -v '^FAIL interface\.assert <assert\.h>:' || true)
    assert_fail=$(grep -c '^FAIL interface\.assert <assert\.h>:' "$log" || true)

    if [ -n "$required" ] || { [ "$assert_fail" -ne 0 ] && ! macro_probe; }; then
        cat "$log"
        return 1
    fi
    if [ "$assert_fail" -ne 0 ]; then
        sed \
            -e 's|^FAIL interface\.assert <assert\.h>:.*$|MACRO interface.assert <assert.h>: function-like macro verified by assert(1)|' \
            -e 's/1 required fail/0 required fail (1 macro)/' "$log"
        status=0
    else
        cat "$log"
    fi
    return "$status"
}

run_sweep x86_64 "$@"
AARCH64_CC=${AARCH64_CC:-aarch64-linux-gnu-gcc}
export AARCH64_CC
run_sweep aarch64 "$@"
