#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 aarch64 net/sshd row over the K194 tap/bridge, with a real host SSH client exchange.
# The QEMU user-net hostfwd path accepts the host TCP connection but never delivers the guest
# SSH banner on this board; the tap path uses the same private netns + dnsmasq DHCP contract as
# the x86 SSH matrix (host 10.0.2.2, dns 10.0.2.3, guest 10.0.2.15).
#
# The helper needs passwordless sudo for the netns/tap/dnsmasq setup; it refuses cleanly without
# it. Everything it creates is deleted on exit.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

image=build/qtest/release-aarch64-k434-r189.img
if [ "${1:-}" = "--image" ] && [ "$#" -ge 2 ]; then
    image=$2
fi
case $image in
    /*) ;;
    *) image=$root/$image ;;
esac
[ -f "$image" ] || {
    echo "aarch64-net-tap: missing image $image" >&2
    exit 2
}

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "aarch64-net-tap: root required for the netns/tap setup (passwordless sudo unavailable); refusing" >&2
        exit 3
    }
    SUDO="sudo -n"
fi

ns=carl434
bridge=carlbr0
tap=carldemo0
host_ip=10.0.2.2
dns_ip=10.0.2.3
guest_ip=10.0.2.15
state=/tmp/opencode/carl-k434-net
conf=$state/dnsmasq.conf
pidfile=$state/dnsmasq.pid
user=$(id -un)
out=build/k434-aarch64-net-tap
mkdir -p "$out" "$state"

cleanup() {
    if [ -f "$pidfile" ]; then
        $SUDO kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
    fi
    $SUDO ip netns del "$ns" >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

$SUDO ip netns del "$ns" >/dev/null 2>&1 || true
$SUDO ip netns add "$ns"
$SUDO ip netns exec "$ns" ip link set lo up
$SUDO ip netns exec "$ns" ip link add "$bridge" type bridge
$SUDO ip netns exec "$ns" ip addr add "$host_ip/24" dev "$bridge"
$SUDO ip netns exec "$ns" ip addr add "$dns_ip/24" dev "$bridge"
$SUDO ip netns exec "$ns" ip link set "$bridge" up
$SUDO ip netns exec "$ns" ip tuntap add dev "$tap" mode tap user "$user"
$SUDO ip netns exec "$ns" ip link set "$tap" master "$bridge"
$SUDO ip netns exec "$ns" ip link set "$tap" up
{
    printf 'interface=%s\n' "$bridge"
    printf 'bind-interfaces\n'
    printf 'listen-address=%s\n' "$dns_ip"
    printf 'dhcp-range=%s,%s,255.255.255.0,12h\n' "$guest_ip" "$guest_ip"
    printf 'dhcp-option=3,%s\n' "$host_ip"
    printf 'dhcp-option=6,%s\n' "$dns_ip"
    printf 'pid-file=%s\n' "$pidfile"
    printf 'log-facility=%s/dnsmasq.log\n' "$state"
} > "$conf"
$SUDO ip netns exec "$ns" dnsmasq --conf-file="$conf"

$SUDO ip netns exec "$ns" runuser -u "$user" -- \
    env RAVEN_SSH_TAP="$tap" RAVEN_SSH_HOST="$guest_ip" \
    python3 "$root/tests/ssh/aarch64-net.py" \
        --image "$image" --tap "$tap" --host "$guest_ip"
