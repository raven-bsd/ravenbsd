#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K228 guest soak workload. The host driver stages this script at /etc/soak-workload.sh and
# sends one command: /bin/brush /etc/soak-workload.sh <cycles> <interval>. Each cycle prints
# unique markers; the host timestamps them and detects a missing cycle-done as a stall.
#
# The workload covers ping, /tmp read/write, fork/exec, the product sync call, and service
# health. It does not hard-code a pass or synthesize a heartbeat.

cycles=${1:-24}
interval=${2:-5}
case $cycles in
    ''|*[!0-9]*) echo "soak-workload: bad cycles: $cycles"; exit 2 ;;
esac
case $interval in
    ''|*[!0-9]*) echo "soak-workload: bad interval: $interval"; exit 2 ;;
esac
if [ "$cycles" -lt 1 ] || [ "$interval" -lt 1 ]; then
    echo "soak-workload: cycles and interval must be positive"
    exit 2
fi

n=0
echo "soak-ready"
while [ "$n" -lt "$cycles" ]; do
    n=$((n + 1))
    echo "soak-cycle: $n"

    /sbin/ping -c 1 -w 2 10.0.2.2
    echo "soak-ping: $?"

    echo "soak-write: $n" > /tmp/soak.txt
    /bin/cat /tmp/soak.txt
    /bin/rm -f /tmp/soak.txt

    /bin/uname > /tmp/soak-uname.txt
    echo "soak-fork: $?"
    /bin/rm -f /tmp/soak-uname.txt

    /bin/soak-sync
    echo "soak-sync: $?"

    /bin/svcctl list
    echo "soak-services: $?"

    echo "soak-cycle-done: $n"
    /bin/sleep "$interval"
done
echo "soak-done"
exit 0
