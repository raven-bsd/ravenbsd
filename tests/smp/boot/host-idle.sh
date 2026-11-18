#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Exclusive-host preflight for the S107 boot-loop soak and kernel gate.
#
# Refuse to start when another QEMU, xtask/cargo/rustc job or a high load average is present.
# --wait keeps polling the same checks in the foreground for the requested number of seconds.
# Host state is always written before the decision (including the refusal).

set -u

wait_secs=0
max_load=2.0
state_file=build/host-idle-state.txt
context=host

usage() {
    cat <<'EOF'
usage: sh tests/smp/boot/host-idle.sh [options]

  --wait SECONDS      wait up to SECONDS for an idle host (default 0: refuse)
  --max-load LOAD     maximum 1-minute load average (default 2.0)
  --state-file PATH   host-state record path (default build/host-idle-state.txt)
  --context NAME      label for the record/log (default host)
  -h, --help          show this help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --wait) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; wait_secs=$2; shift 2 ;;
        --max-load) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; max_load=$2; shift 2 ;;
        --state-file) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; state_file=$2; shift 2 ;;
        --context) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; context=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "host-idle: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

mkdir -p "$(dirname "$state_file")" 2>/dev/null || true

qemu_pattern='^(qemu-system-|/.*qemu-system-)'
job_pattern='^(target/(debug|release)/xtask|cargo (xtask|build|test|run)|rustc|/.*rustc)'

write_state() {
    loadavg=$(cat /proc/loadavg 2>/dev/null || echo 'unknown')
    nproc=$(nproc 2>/dev/null || echo unknown)
    qemu=$(pgrep -fa "$qemu_pattern" 2>/dev/null || true)
    jobs=$(pgrep -fa "$job_pattern" 2>/dev/null || true)
    {
        printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'context=%s\n' "$context"
        printf 'loadavg=%s\n' "$loadavg"
        printf 'nproc=%s\n' "$nproc"
        printf 'max_load=%s\n' "$max_load"
        printf 'wait_secs=%s\n' "$wait_secs"
        printf 'qemu:\n%s\n' "${qemu:-none}"
        printf 'jobs:\n%s\n' "${jobs:-none}"
        printf 'kvm='; ls -l /dev/kvm 2>&1 || true
        printf 'qemu_version='; qemu-system-x86_64 --version 2>&1 | head -n 1 || true
        printf 'free:\n'; free -h 2>/dev/null | head -n 2 || true
    } > "$state_file" 2>/dev/null || true
}

check_idle() {
    loadavg=$(cat /proc/loadavg 2>/dev/null || echo '0')
    load1=${loadavg%% *}
    qemu=$(pgrep -fa "$qemu_pattern" 2>/dev/null || true)
    jobs=$(pgrep -fa "$job_pattern" 2>/dev/null || true)
    idle=1
    busy=    # colon-delimited reasons
    awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l <= m) }' || {
        idle=0
        busy="${busy}load($load1>$max_load) "
    }
    [ -n "$qemu" ] && { idle=0; busy="${busy}qemu "; }
    [ -n "$jobs" ] && { idle=0; busy="${busy}xtask/cargo/rustc "; }
    write_state
}

deadline=$(( $(date +%s) + wait_secs ))
attempt=0
while :; do
    attempt=$((attempt + 1))
    check_idle
    if [ "$idle" -eq 1 ]; then
        echo "host-idle: PASS context=$context load=$load1 max_load=$max_load"
        echo "host-idle: state=$state_file"
        exit 0
    fi
    if [ "$wait_secs" -eq 0 ]; then
        echo "host-idle: REFUSE context=$context busy=$busy (state=$state_file)" >&2
        exit 2
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
        echo "host-idle: TIMEOUT context=$context busy=$busy after ${wait_secs}s (state=$state_file)" >&2
        exit 2
    fi
    if [ "$attempt" -eq 1 ]; then
        echo "host-idle: waiting context=$context busy=$busy (max ${wait_secs}s)"
    fi
    sleep 5
done
