#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# MPTCP2c: two-tap fixture for the QEMU MPTCP failover case.
#
#   qtest-net-two-taps.sh check    verify prerequisites without root (no side effects)
#   qtest-net-two-taps.sh up       create primary/backup bridges and taps, print the contract
#   qtest-net-two-taps.sh down     remove both taps and bridges
#   qtest-net-two-taps.sh status   print the contract while the fixture is up
#   qtest-net-two-taps.sh unplug-primary   take the primary tap down (the failover trigger)
#   qtest-net-two-taps.sh replug-primary   bring the primary tap back up
#
# The primary and backup paths are separate L2 segments, so the host peer can bind one
# address on each and the guest can use two interfaces. No NAT, forwarding or DNS is
# configured: the peer is on the same bridge as the guest. The K194 helper
# (`scripts/qemu-net.sh`) owns the 10.0.2.0/24 demo network and is left untouched.
#
# This helper needs root (it creates links); the caller runs it with `sudo -n` when it is
# not root. A leftover fixture is refused rather than guessed at, and `down` is idempotent.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

state=${RAVEN_MPTCP_STATE:-/run/raven-mptcp-two-tap.state}
primary_bridge=${RAVEN_MPTCP_PRIMARY_BRIDGE:-mptcpbr0}
backup_bridge=${RAVEN_MPTCP_BACKUP_BRIDGE:-mptcpbr1}
primary_tap=${RAVEN_MPTCP_PRIMARY_TAP:-mptcp-tap0}
backup_tap=${RAVEN_MPTCP_BACKUP_TAP:-mptcp-tap1}
primary_host=10.77.1.2
primary_guest=10.77.1.15
backup_host=10.77.2.2
backup_guest=10.77.2.15
prefix=24

usage() {
    cat <<'EOF'
usage:
  scripts/qtest-net-two-taps.sh check           verify prerequisites without root
  scripts/qtest-net-two-taps.sh up              create both bridges and taps
  scripts/qtest-net-two-taps.sh down            remove both bridges and taps
  scripts/qtest-net-two-taps.sh status          print the contract when up
  scripts/qtest-net-two-taps.sh unplug-primary  take the primary tap down
  scripts/qtest-net-two-taps.sh replug-primary  bring the primary tap back up
EOF
}

refuse() {
    printf 'mptcp-two-tap: %s\n' "$*" >&2
    exit 2
}

require_root() {
    [ "$(id -u)" -eq 0 ] || refuse "root required (rerun with sudo)"
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || refuse "required command not found: $1"
}

state_value() {
    key=$1
    [ -f "$state" ] || return 0
    sed -n "s/^$key=//p" "$state" | head -n 1
}

contract() {
    printf 'primary_tap=%s primary_bridge=%s primary_host=%s primary_guest=%s\n' \
        "$(state_value primary_tap)" "$(state_value primary_bridge)" \
        "$primary_host" "$primary_guest"
    printf 'backup_tap=%s backup_bridge=%s backup_host=%s backup_guest=%s\n' \
        "$(state_value backup_tap)" "$(state_value backup_bridge)" \
        "$backup_host" "$backup_guest"
    printf '%s\n' "qemu: -netdev tap,id=n0,ifname=$primary_tap,script=no,downscript=no"
    printf '%s\n' "qemu: -netdev tap,id=n1,ifname=$backup_tap,script=no,downscript=no"
}

check() {
    require_cmd ip
    if ip link show "$primary_bridge" >/dev/null 2>&1 || ip link show "$primary_tap" >/dev/null 2>&1; then
        refuse "leftover $primary_bridge/$primary_tap; run 'scripts/qtest-net-two-taps.sh down'"
    fi
    if ip link show "$backup_bridge" >/dev/null 2>&1 || ip link show "$backup_tap" >/dev/null 2>&1; then
        refuse "leftover $backup_bridge/$backup_tap; run 'scripts/qtest-net-two-taps.sh down'"
    fi
    printf 'mptcp-two-tap: check PASS (ip; %s/%s and %s/%s free)\n' \
        "$primary_bridge" "$primary_tap" "$backup_bridge" "$backup_tap"
}

create_path() {
    bridge=$1
    tap=$2
    host=$3
    ip link add name "$bridge" type bridge
    ip addr add "$host/$prefix" dev "$bridge"
    ip link set "$bridge" up
    ip tuntap add dev "$tap" mode tap user "$user"
    ip link set "$tap" master "$bridge"
    ip link set "$tap" up
}

up() {
    require_root
    require_cmd ip

    if [ -f "$state" ]; then
        contract
        return 0
    fi

    if ip link show "$primary_bridge" >/dev/null 2>&1 || ip link show "$primary_tap" >/dev/null 2>&1; then
        refuse "leftover $primary_bridge/$primary_tap; run 'scripts/qtest-net-two-taps.sh down' first"
    fi
    if ip link show "$backup_bridge" >/dev/null 2>&1 || ip link show "$backup_tap" >/dev/null 2>&1; then
        refuse "leftover $backup_bridge/$backup_tap; run 'scripts/qtest-net-two-taps.sh down' first"
    fi

    user=${SUDO_USER:-$(id -un)}
    printf 'primary_bridge=%s\nprimary_tap=%s\nbackup_bridge=%s\nbackup_tap=%s\nuser=%s\n' \
        "$primary_bridge" "$primary_tap" "$backup_bridge" "$backup_tap" "$user" >"$state"

    partial=1
    trap '[ "$partial" -eq 1 ] && down >/dev/null 2>&1 || true' EXIT HUP INT TERM

    create_path "$primary_bridge" "$primary_tap" "$primary_host"
    create_path "$backup_bridge" "$backup_tap" "$backup_host"

    partial=0
    trap - EXIT HUP INT TERM
    contract
}

down() {
    require_root
    require_cmd ip

    if [ -f "$state" ]; then
        tap0=$(state_value primary_tap)
        tap1=$(state_value backup_tap)
        bridge0=$(state_value primary_bridge)
        bridge1=$(state_value backup_bridge)
        [ -n "$tap0" ] || tap0=$primary_tap
        [ -n "$tap1" ] || tap1=$backup_tap
        [ -n "$bridge0" ] || bridge0=$primary_bridge
        [ -n "$bridge1" ] || bridge1=$backup_bridge
    else
        tap0=$primary_tap
        tap1=$backup_tap
        bridge0=$primary_bridge
        bridge1=$backup_bridge
    fi

    ip link del "$tap0" 2>/dev/null || true
    ip link del "$tap1" 2>/dev/null || true
    ip link del "$bridge0" 2>/dev/null || true
    ip link del "$bridge1" 2>/dev/null || true
    rm -f "$state"
    printf 'mptcp-two-tap: down (%s/%s and %s/%s removed)\n' \
        "$bridge0" "$tap0" "$bridge1" "$tap1"
}

status() {
    if [ -f "$state" ]; then
        contract
    else
        printf 'mptcp-two-tap: not up\n'
    fi
}

unplug_primary() {
    require_root
    require_cmd ip
    tap=$(state_value primary_tap)
    [ -n "$tap" ] || tap=$primary_tap
    ip link set "$tap" down
    printf 'mptcp-two-tap: primary down (%s)\n' "$tap"
}

replug_primary() {
    require_root
    require_cmd ip
    tap=$(state_value primary_tap)
    [ -n "$tap" ] || tap=$primary_tap
    ip link set "$tap" up
    printf 'mptcp-two-tap: primary up (%s)\n' "$tap"
}

command=${1:-}
case "$command" in
    check) check ;;
    up) up ;;
    down) down ;;
    status) status ;;
    unplug-primary) unplug_primary ;;
    replug-primary) replug_primary ;;
    "" | -h | --help) usage ;;
    *) usage >&2; exit 2 ;;
esac
