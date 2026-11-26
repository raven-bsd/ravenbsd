#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K342 TCP pcap regression: replay the committed K281 tcpdump fixtures and the
# K323 loss/reordering pcaps through the decoder and assert the wire outcome.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

command -v tcpdump >/dev/null 2>&1 || {
    echo "tcp-pcap: tcpdump is required" >&2
    exit 77
}
command -v python3 >/dev/null 2>&1 || {
    echo "tcp-pcap: python3 is required" >&2
    exit 77
}

timeout 240 python3 tests/tcp-pcap-regression/run.py
