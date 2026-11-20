#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K194: bidirectional tap/bridge network for QEMU demos, plus the original S95
# reproducible user-network gate when called without a subcommand.
#
#   qemu-net.sh up      create ravenbr0/ravendemo0, NAT and dnsmasq; print the contract
#   qemu-net.sh down    remove them and restore ip_forward
#   qemu-net.sh status  print the contract if the helper is up
#   qemu-net.sh         run the S95 slirp user-network gate
#
# The normal path needs root; the helper refuses cleanly without it. It never
# guesses the uplink: it reads the IPv4 default route and verifies the device.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

bridge=${RAVEN_QEMU_BRIDGE:-ravenbr0}
tap=${RAVEN_QEMU_TAP:-ravendemo0}
state=/run/raven-qemu-net.state
dnsmasq_pid=/run/raven-qemu-net-dnsmasq.pid
dnsmasq_conf=/run/raven-qemu-net-dnsmasq.conf
dnsmasq_log=/run/raven-qemu-net-dnsmasq.log

usage() {
    cat <<'EOF'
usage:
  scripts/qemu-net.sh --check  verify prerequisites without root or booting
  scripts/qemu-net.sh up       create the bridge/tap, NAT and DNS; print the contract
  scripts/qemu-net.sh down     remove them and restore ip_forward
  scripts/qemu-net.sh status   print the contract if up
  scripts/qemu-net.sh          run the S95 slirp user-network gate
EOF
}

check() {
    for command in ip iptables dnsmasq sysctl; do
        command -v "$command" >/dev/null 2>&1 || refuse "missing prerequisite: $command"
    done
    uplink=$(uplink_device)
    [ -n "$uplink" ] || refuse "no IPv4 default route"
    ip link show "$uplink" >/dev/null 2>&1 || refuse "uplink $uplink does not exist"
    if ip link show "$bridge" >/dev/null 2>&1 || ip link show "$tap" >/dev/null 2>&1; then
        refuse "leftover $bridge/$tap; run 'scripts/qemu-net.sh down'"
    fi
    printf 'qemu-net: check PASS (ip, iptables, dnsmasq, sysctl; uplink=%s; %s/%s free)\n' \
        "$uplink" "$bridge" "$tap"
}

refuse() {
    printf 'qemu-net: %s\n' "$*" >&2
    exit 2
}

require_root() {
    [ "$(id -u)" -eq 0 ] || refuse "root required (rerun with sudo)"
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || refuse "required command not found: $1"
}

uplink_device() {
    ip -4 route show default | awk '/^default/ {for (i = 1; i <= NF; i++) if ($i == "dev") {print $(i + 1); exit}}'
}

contract() {
    printf 'tap=%s bridge=%s guest=10.0.2.15 host=10.0.2.2 dns=10.0.2.3 uplink=%s\n' \
        "$tap" "$bridge" "${1:-unknown}"
    printf '%s\n' "qemu: -netdev tap,id=n0,ifname=$tap,script=no,downscript=no -device virtio-net-pci,netdev=n0,disable-legacy=on"
}

up() {
    require_root
    require_cmd ip
    require_cmd iptables
    require_cmd dnsmasq
    require_cmd sysctl

    if [ -f "$state" ]; then
        # The state file is the single source of truth for a live helper.
        # shellcheck disable=SC1090
        . "$state"
        contract "${uplink:-unknown}"
        return 0
    fi

    uplink=$(uplink_device)
    [ -n "$uplink" ] || refuse "no IPv4 default route; refusing to guess the host uplink"
    ip link show "$uplink" >/dev/null 2>&1 || refuse "uplink $uplink does not exist"
    [ "$uplink" != "$bridge" ] || refuse "default route is the bridge itself"

    if ip link show "$bridge" >/dev/null 2>&1 || ip link show "$tap" >/dev/null 2>&1; then
        refuse "leftover $bridge/$tap; run 'scripts/qemu-net.sh down' first"
    fi

    user=${SUDO_USER:-$(id -un)}
    prev_forward=$(sysctl -n net.ipv4.ip_forward)
    printf 'bridge=%s\ntap=%s\nuser=%s\nuplink=%s\nprev_forward=%s\n' \
        "$bridge" "$tap" "$user" "$uplink" "$prev_forward" >"$state"

    # A failure after this point calls down() through the trap so no partial
    # bridge, tap, dnsmasq or NAT rule is left behind.
    partial=1
    trap '[ "$partial" -eq 1 ] && down >/dev/null 2>&1 || true' EXIT HUP INT TERM

    ip link add name "$bridge" type bridge
    ip addr add 10.0.2.2/24 dev "$bridge"
    ip addr add 10.0.2.3/24 dev "$bridge" 2>/dev/null || true
    ip link set "$bridge" up

    ip tuntap add dev "$tap" mode tap user "$user"
    ip link set "$tap" master "$bridge"
    ip link set "$tap" up

    sysctl -q -w net.ipv4.ip_forward=1

    iptables -t nat -N RAVENBAKER-NAT
    iptables -t nat -A RAVENBAKER-NAT -s 10.0.2.0/24 ! -o "$bridge" -j MASQUERADE
    iptables -t nat -C POSTROUTING -s 10.0.2.0/24 -j RAVENBAKER-NAT 2>/dev/null \
        || iptables -t nat -A POSTROUTING -s 10.0.2.0/24 -j RAVENBAKER-NAT

    iptables -N RAVENBAKER-FWD
    iptables -A RAVENBAKER-FWD -i "$bridge" -j ACCEPT
    iptables -A RAVENBAKER-FWD -o "$bridge" -j ACCEPT
    iptables -C FORWARD -j RAVENBAKER-FWD 2>/dev/null \
        || iptables -A FORWARD -j RAVENBAKER-FWD

    cat >"$dnsmasq_conf" <<EOF
interface=$bridge
bind-interfaces
listen-address=10.0.2.3
dhcp-range=10.0.2.15,10.0.2.15,255.255.255.0,12h
dhcp-option=3,10.0.2.2
dhcp-option=6,10.0.2.3
pid-file=$dnsmasq_pid
log-facility=$dnsmasq_log
EOF
    dnsmasq --conf-file="$dnsmasq_conf"
    [ -s "$dnsmasq_pid" ] || refuse "dnsmasq did not start on 10.0.2.3"

    partial=0
    trap - EXIT HUP INT TERM
    contract "$uplink"
}

down() {
    require_root
    require_cmd ip
    require_cmd iptables

    if [ -f "$state" ]; then
        # shellcheck disable=SC1090
        . "$state"
    fi

    if [ -s "$dnsmasq_pid" ]; then
        kill "$(cat "$dnsmasq_pid")" 2>/dev/null || true
    fi
    pkill -f "dnsmasq --conf-file=$dnsmasq_conf" 2>/dev/null || true

    iptables -D FORWARD -j RAVENBAKER-FWD 2>/dev/null || true
    iptables -F RAVENBAKER-FWD 2>/dev/null || true
    iptables -X RAVENBAKER-FWD 2>/dev/null || true
    iptables -t nat -D POSTROUTING -s 10.0.2.0/24 -j RAVENBAKER-NAT 2>/dev/null || true
    iptables -t nat -F RAVENBAKER-NAT 2>/dev/null || true
    iptables -t nat -X RAVENBAKER-NAT 2>/dev/null || true

    if [ -n "${prev_forward:-}" ]; then
        sysctl -q -w "net.ipv4.ip_forward=$prev_forward" || true
    fi

    ip link del "$tap" 2>/dev/null || true
    ip link del "$bridge" 2>/dev/null || true
    rm -f "$state" "$dnsmasq_conf" "$dnsmasq_log" "$dnsmasq_pid"
    printf 'qemu-net: down (%s/%s removed)\n' "$bridge" "$tap"
}

status() {
    if [ -f "$state" ]; then
        # shellcheck disable=SC1090
        . "$state"
        contract "${uplink:-unknown}"
    else
        printf 'qemu-net: not up\n'
    fi
}

# S95 gate: preserved for the existing reproducible user-network check.
gate() {
    arch=${RAVEN_QT_ARCH:-x86_64}
    case "$arch" in
        x86_64) ;;
        *)
            echo "qemu-net: only x86_64 is supported, got $arch" >&2
            exit 2
            ;;
    esac

    printf '%s\n' \
        'qemu-net: args: -nic none -netdev user,id=n0 -device virtio-net-pci,netdev=n0,disable-legacy=on'

    cargo xtask qtest --case network --arch "$arch"

    log=build/qtest/network.log
    [ -f "$log" ] || {
        echo "qemu-net: missing $log" >&2
        exit 1
    }

    grep -q '^netd: interface eth0 10\.0\.2\.15/24$' "$log"
    grep -q '^ping: 10\.0\.2\.2 seq=1 rtt=' "$log"
    grep -q '^ping: [1-9][0-9]* packets transmitted, [1-9][0-9]* received, 0% packet loss$' "$log"
    if grep -q 'netd:.*nic:info.*no such file' "$log"; then
        echo "qemu-net: netd nic:info failure present" >&2
        exit 1
    fi

    printf 'qemu-net: pass (%s)\n' "$log"
}

case "${1:-}" in
    --check|check) check ;;
    up) shift; up "$@" ;;
    down) shift; down "$@" ;;
    status) shift; status "$@" ;;
    "") gate "$@" ;;
    -h|--help) usage ;;
    *) usage >&2; exit 2 ;;
esac
