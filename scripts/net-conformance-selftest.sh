#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K253/K275 harness self-test: frame build/parse, checksums, TCP options, pcap
# and the matrix runner's parsing/label rules.  No tap, root or guest image.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

python3 -m py_compile \
    tests/net-conformance/harness.py \
    tests/net-conformance/linux_server.py \
    tests/net-conformance/harness/run.py
sh -n tests/fixtures/net-conformance/build.sh
sh -n scripts/net-conformance.sh
python3 tests/net-conformance/harness.py --selftest
python3 tests/net-conformance/harness/run.py --selftest
