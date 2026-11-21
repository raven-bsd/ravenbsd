#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K388: the exact commands from docs/releases/0.1.0-soak-runbook.md, extracted so the
# Markdown can be verified with `sh -n` (the Markdown itself is not shell).
#
# THIS FILE IS NOT RUN BY K388. The 24 h gate is owner-only and needs a quiet host.
# Running any of these commands without arming the runbook is refused.

set -eu

if [ "${SOAK_RUNBOOK_ARM:-0}" != "1" ]; then
    echo "runbook-commands: refusing to run; export SOAK_RUNBOOK_ARM=1 to arm the owner 24 h run" >&2
    exit 3
fi

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

# ---- pre-flight (light checks; none is the gate) -------------------------------------------
timeout 60 sh scripts/soak.sh --check
timeout 600 make soak-smoke

# ---- run identity (record before starting) -------------------------------------------------
commit=$(git rev-parse HEAD)
image=build/soak/image.img
image_sha256=$(sha256sum "$image" | awk '{print $1}')
date -u +%Y-%m-%dT%H:%M:%SZ

# ---- the gate: 24 h soak (86400 s workload + 30 s interval, 3600 s margin) -----------------
timeout 90000 make soak-24h

# ---- hourly checkpoint (from a second terminal while the gate runs) ------------------------
hour=00
run=$(ls -dt build/soak/run-* | head -1)
date -u +%Y-%m-%dT%H:%M:%SZ
cat /proc/loadavg
pgrep -a qemu-system-x86_64
ps -o pid,etime,%cpu,rss,cmd -C qemu-system-x86_64
wc -c "$run"/soak.jsonl "$run"/serial.log "$run"/serial.stamped.log
grep -Ei 'panic|EXCEPTION|fatal|trap|watchdog|stall' "$run"/serial.log | tail -n 20
dmesg | grep -Ei 'oom|killed process|qemu-system' | tail -n 20
out=tests/evidence/soak/$image_sha256
mkdir -p "$out"
cp -f /proc/loadavg "$out"/hour-$hour.log

# ---- publish the evidence (after the run, or after an abort) -------------------------------
mkdir -p "$out"
cp -f "$run"/summary.json "$run"/summary.txt "$out"/ 2>/dev/null || true
cd "$out"
sha256sum ./* > SHA256SUMS
