#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 per-step SSH gate matrix (S0-S8). Runs the pty-alloc qtest, builds the throw-away S0 echo
# initfs, then runs matrix.py inside a private network namespace with its own ravenbr0/ravendemo0
# (guest 10.0.2.15). The namespace keeps other lanes' 10.0.2.0/24 bridges from stealing the route.
# Needs passwordless sudo; refuses cleanly otherwise.
#
# Usage: sh tests/ssh/matrix.sh
# Outer acceptance timeout: 900 s; the inner commands have their own proportional caps.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

bridge=ravenbr0
tap=ravendemo0
ns=ken434
host_ip=10.0.2.2
dns_ip=10.0.2.3
guest_ip=10.0.2.15
state=/tmp/opencode/ken-k434-net
conf=$state/dnsmasq.conf
pidfile=$state/dnsmasq.pid
user=$(id -un)

if [ "$(id -u)" -eq 0 ]; then
    SUDO=
else
    sudo -n true 2>/dev/null || {
        echo "matrix: root required for the private tap namespace; refusing" >&2
        exit 3
    }
    SUDO="sudo -n"
fi

up=0
down() {
    if [ "$up" -eq 1 ]; then
        if [ -f "$pidfile" ]; then
            $SUDO kill "$(cat "$pidfile")" >/dev/null 2>&1 || true
        fi
        $SUDO ip netns del "$ns" >/dev/null 2>&1 || true
        up=0
    fi
}
trap down EXIT HUP INT TERM

# S8's dynamic pty case comes from merged K430. Its measured run after K442 is ~39 s; 75 s is a
# small margin.
mkdir -p build/ssh/matrix "$state"
echo "matrix: running the pty-alloc qtest" >&2
pty_exit=0
timeout 75 cargo xtask qtest --case pty-alloc --arch x86_64 \
    > build/ssh/matrix/pty-alloc.log 2>&1 || pty_exit=$?
echo "$pty_exit" > build/ssh/matrix/pty-alloc.exit

# S0 is a separate throw-away `server` initfs. A build failure is reported by the S0 row; it does
# not hide the SSH rows.
echo "matrix: building the S0 echo fixture" >&2
timeout 120 sh tests/fixtures/ssh-echo/build.sh || echo "matrix: S0 fixture build failed" >&2

echo "matrix: private netns $ns bridge=$bridge tap=$tap guest=$guest_ip" >&2
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
    printf 'log-facility=%s\n' "$state/dnsmasq.log"
} > "$conf"
$SUDO ip netns exec "$ns" dnsmasq --conf-file="$conf"
up=1

# Run the Python driver (and its QEMU/ssh children) inside the namespace as the invoking user, so
# the tap owner and build artifacts stay consistent.
timeout 780 $SUDO ip netns exec "$ns" runuser -u "$user" -- \
    env RAVEN_SSH_HOST="$guest_ip" RAVEN_QEMU_TAP="$tap" \
    python3 "$root/tests/ssh/matrix.py"
