#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Product-side mirror of the prjmgmt `scripts/qemu-hygiene.sh` check used before a
# latency-sensitive boot run. It refuses a host with more than two qemu-system processes or
# load1 > 8; it does not kill other lanes' VMs.

set -eu

max_qemu=2
max_load=8

count=$(pgrep -c -x qemu-system-x86 2>/dev/null || true)
[ -n "$count" ] || count=0
load1=$(cut -d' ' -f1 /proc/loadavg)
printf 'coldstart-hygiene: qemu=%s load1=%s\n' "$count" "$load1"

if [ "$count" -gt "$max_qemu" ]; then
    printf 'coldstart-hygiene: REFUSE qemu=%s > %s; wait for a quiet window\n' \
        "$count" "$max_qemu" >&2
    exit 1
fi
load_int=${load1%%.*}
if [ "$load_int" -gt "$max_load" ]; then
    printf 'coldstart-hygiene: REFUSE load1=%s > %s; wait for a quiet window\n' \
        "$load1" "$max_load" >&2
    exit 1
fi
printf 'coldstart-hygiene: OK\n'
