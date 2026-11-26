#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K408 tap wrapper. Bring up the K194 bridge/tap, run the accepted-socket qtest with the host peer
# on the tap, and always tear the tap down. The qtest case itself is reusable when the tap is
# already up (for example inside the K195 gate).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

cleanup() {
    timeout 30 sudo -n sh scripts/qemu-net.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

timeout 30 sudo -n sh scripts/qemu-net.sh up
timeout 240 cargo xtask qtest --case socket-write-exec-tap --arch x86_64
