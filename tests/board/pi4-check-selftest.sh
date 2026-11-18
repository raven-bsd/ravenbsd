#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Self-test for scripts/board-pi4-check.sh. It builds temporary capture logs in a
# scratch directory, never touches a serial device and never claims a board result.
#
# Usage: sh tests/board/pi4-check-selftest.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
check=$root/scripts/board-pi4-check.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/board-pi4-check.XXXXXX")
trap 'rm -rf "$work"' EXIT

failures=0

pass() {
    printf 'ok   %s\n' "$1"
}

fail() {
    printf 'FAIL %s\n' "$1" >&2
    failures=$((failures + 1))
}

run_expect() {
    name=$1
    expected=$2
    shift 2
    set +e
    output=$("$@" 2>&1)
    code=$?
    set -e
    if [ "$code" -eq "$expected" ]; then
        pass "$name (exit $code)"
    else
        printf '%s\n' "$output" >&2
        fail "$name: expected exit $expected, got $code"
        return 1
    fi
}

make_board_log() {
    cat > "$1" <<'EOF'
# Pi 4 serial capture
# started: 2026-11-18T00:00:00Z
# host: Linux 6.0 aarch64
# commit: 0123456789abcdef0123456789abcdef01234567
# image: build/aarch64/ravenbsd-pi4.img
# image sha256: 0a3e917d45e80d38c2c2e842639bca77a38a6457c86bfd4b03a254d7cf8c64d4
# device: /dev/ttyUSB0 at 115200 8N1
# host dynamic: 0x0000000000000001 (NEEDED) Shared library: [libc.so.1]
RavenBSD kernel 0.1 (aarch64)
devd: dtb launch bcm2711-rpi-4-b
login:
Password:
raven# /bin/uname -a
RavenBSD pi4 0.1 2026-11-18 aarch64
raven# readelf -d /bin/rsh | grep NEEDED
 0x0000000000000001 (NEEDED)             Shared library: [libc.so.1]
genet: link is up 1000 Mbps full duplex
netd: DHCP lease 192.168.1.50/24 gateway 192.168.1.1
raven# ping -c 3 192.168.1.1
64 bytes from 192.168.1.1: icmp_seq=1 ttl=64 time=0.512 ms
init exited (0)
EOF
}

make_board_log "$work/board.log"

# 1. A complete board-shaped transcript is accepted, but the checker must not
# claim a board PASS.
run_expect "complete transcript" 0 "$check" --log "$work/board.log"
set +e
output=$("$check" --log "$work/board.log" 2>&1)
set -e
if printf '%s\n' "$output" | grep -Fq 'transcript markers complete'; then
    pass "complete marker summary"
else
    fail "complete marker summary missing"
fi
if printf '%s\n' "$output" | grep -Fq 'no board PASS is claimed'; then
    pass "no PASS claim"
else
    fail "checker did not state that no board PASS is claimed"
fi

# 2. A QEMU/virt log is refused even with every textual marker.
{
    cat "$work/board.log"
    printf 'QEMU Virtual CPU version 8.2.0\n'
} > "$work/virt.log"
run_expect "virt log refused" 2 "$check" --log "$work/virt.log"

# 3. An explicitly synthetic header is refused.
sed 's/^# host:.*/# source: synthetic fixture/' "$work/board.log" > "$work/synthetic.log"
run_expect "synthetic log refused" 2 "$check" --log "$work/synthetic.log"

# 4. A missing board marker is refused.
sed 's/bcm2711-rpi-4-b/generic aarch64/' "$work/board.log" > "$work/noboard.log"
run_expect "missing board marker refused" 2 "$check" --log "$work/noboard.log"

# 5. A log without the ping reply is incomplete, not a pass.
grep -v 'bytes from' "$work/board.log" > "$work/noping.log"
run_expect "missing ping is incomplete" 1 "$check" --log "$work/noping.log"

# 6. Dry run does not need the log to exist and claims nothing.
run_expect "dry run" 0 "$check" --log "$work/missing.log" --dry-run
set +e
output=$("$check" --log "$work/missing.log" --dry-run 2>&1)
set -e
if printf '%s\n' "$output" | grep -Fq 'dry run only'; then
    pass "dry run states no board result"
else
    fail "dry run did not state that no board result is claimed"
fi

# 7. Help always works.
run_expect "help" 0 "$check" --help

if [ "$failures" -ne 0 ]; then
    echo "board-pi4-check selftest: FAILED ($failures)" >&2
    exit 1
fi

echo "board-pi4-check selftest: PASSED (checker behaviour only; no board result)"
