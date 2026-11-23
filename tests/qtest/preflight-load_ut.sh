#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Rev 111 synthetic-load test for the kernel-timer preflight.
# 4.59 is the highest load1 in the 30 safe runs; 6.32 and 60.83 are the preserved full-gate
# failures. The guard must allow <= 5.0 and refuse above it.

set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
script=$here/preflight-load.sh

check() {
    load=$1
    expected=$2
    if sh "$script" --load1 "$load" >/dev/null 2>&1; then
        actual=0
    else
        actual=$?
    fi
    if [ "$actual" -ne "$expected" ]; then
        echo "preflight-load_ut: FAIL load1=$load rc=$actual expected=$expected"
        exit 1
    fi
    echo "preflight-load_ut: load1=$load rc=$actual expected=$expected ok"
}

check 4.59 0
check 5.00 0
check 5.01 77
check 6.32 77
check 60.83 77
check invalid 2

echo "preflight-load_ut: ok"
