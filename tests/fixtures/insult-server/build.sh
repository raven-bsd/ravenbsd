#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K201: start the deterministic insult content server for the eink-insult qtest.
# It binds 127.0.0.1:18081 for QEMU user-net (guest 10.0.2.2), serves at most 8 requests and
# exits on idle. A previous pid is stopped first so a rerun cannot reuse a stale server.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/insult-server
mkdir -p "$out"
pid_file=$out/server.pid
log=$out/server.log

if [ -f "$pid_file" ]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
    rm -f "$pid_file"
fi
rm -f "$log"
nohup python3 "$root/tests/fixtures/insult-server/server.py" \
    --port 18081 --max-requests 8 --idle-timeout 300 >"$log" 2>&1 &
echo $! >"$pid_file"
sleep 0.3
if ! kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "insult-server: failed to start" >&2
    cat "$log" >&2 || true
    exit 1
fi
echo "insult-server: pid $(cat "$pid_file") on 127.0.0.1:18081"
