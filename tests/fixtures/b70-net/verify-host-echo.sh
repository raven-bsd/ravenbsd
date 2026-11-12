#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-side oracle between the two B70 boots: the first boot's host echo client must have
# received the exact payload. A missing or mismatched artifact fails with the host log.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/b70-net
result=$out/host-echo.result
log=$out/host-echo.log
expected='ok payload=b70-host-echo-payload'

if [ ! -f "$result" ]; then
    echo "b70: host echo result missing: $result" >&2
    [ -f "$log" ] && cat "$log" >&2
    exit 1
fi

actual=$(cat "$result")
if [ "$actual" != "$expected" ]; then
    echo "b70: host echo mismatch: expected '$expected' got '$actual'" >&2
    [ -f "$log" ] && cat "$log" >&2
    exit 1
fi

echo "b70: host echo verified"
