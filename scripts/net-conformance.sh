#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K253 packet-level TCP conformance: run the raw-frame scenarios against the
# host kernel (reference) or the Raven guest over tap.  A TAP device needs
# CAP_NET_ADMIN, so the script re-executes itself in a disposable user+net+mount
# namespace.  That also isolates the Linux baseline from the host network.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

if [ "${NETCONF_NAMESPACE:-0}" != "1" ]; then
    if [ "$(id -u)" -eq 0 ]; then
        exec env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    if unshare -Urnm true 2>/dev/null; then
        exec unshare -Urnm env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    echo "net-conformance: need root or unprivileged user+net+mount namespaces for tap" >&2
    exit 77
fi

build=1
args=
for arg in "$@"; do
    case "$arg" in
        --no-build) build=0 ;;
        *) args="$args $arg" ;;
    esac
done

target=0
baseline=0
for arg in "$@"; do
    case "$arg" in
        --target) target=1 ;;
        --baseline) baseline=1 ;;
    esac
done
if [ "$target" -eq 0 ] && [ "$baseline" -eq 0 ]; then
    echo "net-conformance: choose --baseline linux or --target raven" >&2
    exit 2
fi

command -v python3 >/dev/null 2>&1 || {
    echo "net-conformance: python3 is required" >&2
    exit 77
}

if [ "$target" -eq 1 ]; then
    for command in ip iptables qemu-system-x86_64; do
        command -v "$command" >/dev/null 2>&1 || {
            echo "net-conformance: missing prerequisite: $command" >&2
            exit 77
        }
    done
    if [ ! -f /usr/share/OVMF/OVMF_CODE_4M.fd ] || [ ! -f /usr/share/OVMF/OVMF_VARS_4M.fd ]; then
        echo "net-conformance: OVMF 4M firmware is required" >&2
        exit 77
    fi
    if [ "$build" -eq 1 ]; then
        sh tests/fixtures/net-conformance/build.sh
        cargo xtask image --arch x86_64 --profile network-config-test \
            --initfs build/fixtures/net-conformance/initfs.img \
            --output build/qtest/net-conformance.img
    fi
    tap=${NETCONF_TAP:-nctap0}
    ip link del "$tap" 2>/dev/null || true
    ip tuntap add dev "$tap" mode tap
    ip addr add 10.0.2.2/24 dev "$tap"
    ip link set "$tap" up
    # The host kernel has no socket for the raw connection; its unexpected RST
    # would abort the guest connection before the wire assertion.
    iptables -I OUTPUT -p tcp --dst 10.0.2.15 --tcp-flags RST RST -j DROP
fi

# shellcheck disable=SC2086
python3 tests/net-conformance/harness.py $args
