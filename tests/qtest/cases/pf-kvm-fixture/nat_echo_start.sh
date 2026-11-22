#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K296: start the one-shot UDP NAT echo used by the pf-kvm cases. The qtest pre-hook waits
# for this shell to exit; the server is backgrounded and exits after one datagram.

set -eu

PORT=${1:-47004}
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

pkill -f "pf-kvm-fixture/nat_echo.py $PORT" 2>/dev/null || true
python3 "$HERE/nat_echo.py" "$PORT" >/tmp/k296-nat-echo.log 2>&1 &
