#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# MPTCP2c fixture pre-step: bring up the two taps, start the reaper that removes them when QEMU
# exits, and start the host peer on both bridge addresses. Runs before the qtest image build, so
# the background processes redirect all output and exit immediately.
#
# Usage: start.sh negative|positive

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-negative}
case "$mode" in
    negative | positive) ;;
    *)
        echo "usage: start.sh negative|positive" >&2
        exit 2
        ;;
esac

out=build/fixtures/mptcp
mkdir -p "$out"
qmp=$root/build/qtest/mptcp-qmp.sock
harness_qmp=$root/build/qtest/mptcp-qmp-harness.sock

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    if ! sudo -n true 2>/dev/null; then
        echo "mptcp fixture: root required (passwordless sudo unavailable); refusing" >&2
        exit 3
    fi
    SUDO="sudo -n"
fi

# A failed previous run must not leave a peer, a reaper or a link behind.
pkill -f "tests/fixtures/mptcp/host.py" 2>/dev/null || true
pkill -f "tests/fixtures/mptcp/reaper.sh" 2>/dev/null || true
$SUDO sh scripts/mptcp-two-tap.sh down >/dev/null 2>&1 || true
rm -f "$qmp" "$harness_qmp" "$out/host.ready" "$out/host.result" "$out/host.log" \
    "$out/host.stdout" "$out/reaper.log" "$out/taps.txt"

$SUDO sh scripts/mptcp-two-tap.sh up | tee "$out/taps.txt"

sh tests/fixtures/mptcp/reaper.sh "$root" >"$out/reaper.log" 2>&1 &

if [ "$mode" = negative ]; then
    host_mode=negative
    host_timeout=300
else
    host_mode=listen
    host_timeout=900
fi

python3 tests/fixtures/mptcp/host.py \
    --mode "$host_mode" \
    --qmp "$qmp" \
    --result "$out/host.result" \
    --log "$out/host.log" \
    --ready "$out/host.ready" \
    --timeout "$host_timeout" \
    >"$out/host.stdout" 2>&1 &

attempts=0
while [ ! -f "$out/host.ready" ] && [ "$attempts" -lt 50 ]; do
    sleep 0.2
    attempts=$((attempts + 1))
done
if [ ! -f "$out/host.ready" ]; then
    echo "mptcp fixture: host peer did not become ready" >&2
    cat "$out/host.stdout" >&2 || true
    exit 1
fi
cat "$out/host.ready"

if [ "$mode" = positive ] && ! grep -q 'protocol=mptcp' "$out/host.ready"; then
    echo "mptcp fixture: host MPTCP stack is unavailable; the positive case needs an MPTCP peer" >&2
    exit 3
fi
