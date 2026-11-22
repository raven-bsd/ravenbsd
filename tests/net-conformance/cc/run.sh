#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K289 CUBIC/RACK packet-level runner: same tap/namespace plumbing as the K253
# harness, with the cc bulk fixture.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

if [ "${NETCONF_NAMESPACE:-0}" != "1" ]; then
    if [ "$(id -u)" -eq 0 ]; then
        exec env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    if unshare -Urnm true 2>/dev/null; then
        exec unshare -Urnm env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    echo "cc-conformance: need root or unprivileged user+net+mount namespaces for tap" >&2
    exit 77
fi

for command in ip iptables qemu-system-x86_64 python3; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "cc-conformance: missing prerequisite: $command" >&2
        exit 77
    }
done
if [ ! -f /usr/share/OVMF/OVMF_CODE_4M.fd ] || [ ! -f /usr/share/OVMF/OVMF_VARS_4M.fd ]; then
    echo "cc-conformance: OVMF 4M firmware is required" >&2
    exit 77
fi

if [ "${CC_NO_BUILD:-0}" != "1" ]; then
    sh tests/net-conformance/cc/build.sh
fi

tap=${NETCONF_TAP:-nctap0}
ip link del "$tap" 2>/dev/null || true
ip tuntap add dev "$tap" mode tap
ip addr add 10.0.2.2/24 dev "$tap"
ip link set "$tap" up
iptables -I OUTPUT -p tcp --dst 10.0.2.15 --tcp-flags RST RST -j DROP

python3 tests/net-conformance/cc/run.py "$@"
