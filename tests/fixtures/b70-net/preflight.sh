#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Preflight for the B70 end-to-end case. The guest stack/tools/resolver are owned by
# B12b/B12c/B14/B15; this reports the exact missing artifacts instead of failing inside an
# image build with an unrelated package error.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

missing=""
for path in \
    tests/qtest/cases/net-b.toml \
    tests/qtest/cases/netd-virtio.toml \
    tests/qtest/cases/net-tools.toml \
    tests/qtest/cases/resolv.toml \
    userland/servers/resolvd \
    userland/bin/ping \
    userland/bin/nc \
    userland/bin/netstat
do
    if [ ! -e "$path" ]; then
        missing="$missing $path"
    fi
done

if [ -n "$missing" ]; then
    echo "b70: blocked: missing$missing" >&2
    echo "b70: B12b/B12c/B14/B15 are not merged; the hw-net assertions are parked" >&2
    exit 1
fi
