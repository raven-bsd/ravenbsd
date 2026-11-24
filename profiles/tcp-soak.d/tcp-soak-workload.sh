#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K285 guest TCP workload: sustained nc echo cycles over the loopback netd path, driving
# the merged FreeBSD-derived TCP code (connect/send/recv/close per cycle). The host driver
# (scripts/tcp-soak-driver.py) sends exactly one command:
#
#   /bin/brush /etc/tcp-soak-workload.sh <cycles> <interval> [port]
#
# Each cycle starts one `nc` echo listener in the background, runs one client
# connect/send/recv/close round trip against it, reaps the listener, snapshots the socket
# table with netstat, and prints unique markers the host timestamps. A missing cycle-done
# is a stall to the host; the workload never hard-codes a pass or synthesizes a heartbeat.

cycles=${1:-60}
interval=${2:-10}
port=${3:-7000}
case $cycles in
    ''|*[!0-9]*) echo "tcp-soak-workload: bad cycles: $cycles"; exit 2 ;;
esac
case $interval in
    ''|*[!0-9]*) echo "tcp-soak-workload: bad interval: $interval"; exit 2 ;;
esac
case $port in
    ''|*[!0-9]*) echo "tcp-soak-workload: bad port: $port"; exit 2 ;;
esac
if [ "$cycles" -lt 1 ] || [ "$interval" -lt 1 ]; then
    echo "tcp-soak-workload: cycles and interval must be positive"
    exit 2
fi
if [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
    echo "tcp-soak-workload: port must be 1..65535"
    exit 2
fi

# Fixed 14-byte payload; `nc` echoes exactly what it received back.
payload="raven-tcp-soak"

echo "tcp-ready"
n=0
while [ "$n" -lt "$cycles" ]; do
    n=$((n + 1))
    # Rotate the listener port each cycle: the socket that closed first sits in
    # TIME_WAIT on its port and a fresh listener on the same port fails with
    # "address in use" until the wait expires.
    p=$(( port + n % 50 ))
    echo "tcp-cycle: $n"
    echo "tcp-port: $p"

    /sbin/nc -l -p "$p" -e -w 30 &
    /bin/sleep 1
    /sbin/nc -s "$payload" -n 14 -w 30 127.0.0.1 "$p"
    echo "tcp-echo: $?"
    wait
    echo "tcp-wait: $?"

    /sbin/netstat

    echo "tcp-cycle-done: $n"
    /bin/sleep "$interval"
done
echo "tcp-done"
exit 0
