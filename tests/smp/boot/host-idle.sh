#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Exclusive-host preflight for the S107 boot-loop soak and kernel gate.
#
# Refuse to start when another QEMU, xtask/cargo/rustc job or a high load average is present.
# --allow-jobs ignores co-tenant cargo/rustc/xtask builds: a functional boot row is not
# invalidated by another lane compiling, only by another QEMU or a load above the budget.
# Latency and stress rows keep the strict check (no other job at all).
# --wait keeps polling the same checks in the foreground for the requested number of seconds.
# Host state is always written before the decision (including the refusal).

set -u

wait_secs=0
allow_jobs=0
allow_qemu=0
default_max_load() {
    n=$(nproc 2>/dev/null || echo 4)
    case "$n" in ''|*[!0-9]*) n=4 ;; esac
    m=$((n / 4))
    [ "$m" -lt 2 ] && m=2
    echo "$m"
}
max_load=$(default_max_load)
state_file=build/host-idle-state.txt
context=host

usage() {
    cat <<'EOF'
usage: sh tests/smp/boot/host-idle.sh [options]

  --wait SECONDS      wait up to SECONDS for an idle host (default 0: refuse)
  --max-load LOAD     maximum 1-minute load average (default max(2, nproc/4))
  --allow-jobs        ignore co-tenant cargo/rustc/xtask builds (functional rows); another
                      QEMU and the load budget still refuse
  --allow-qemu N      tolerate up to N other QEMU processes (functional rows use nproc/2;
                      latency rows use 0)
  --state-file PATH   host-state record path (default build/host-idle-state.txt)
  --context NAME      label for the record/log (default host)
  -h, --help          show this help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --wait) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; wait_secs=$2; shift 2 ;;
        --max-load) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; max_load=$2; shift 2 ;;
        --allow-jobs) allow_jobs=1; shift ;;
        --allow-qemu) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; allow_qemu=$2; shift 2 ;;
        --state-file) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; state_file=$2; shift 2 ;;
        --context) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; context=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "host-idle: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

mkdir -p "$(dirname "$state_file")" 2>/dev/null || true

# Match the executable name (`comm`), not the full command line. A shell whose command line
# happens to contain "qemu-system" or "cargo" must not block the gate, and a zombie (<defunct>)
# is not a competing job. `ps stat` is `Z`/`X` for zombies/dead processes.
list_qemu() {
    ps -eo pid=,stat=,comm=,args= 2>/dev/null | awk '$2 !~ /^[ZX]/ && $3 ~ /^qemu-system-/'
}

list_jobs() {
    # A stopped (T/t) build is not progressing either; only running/waiting jobs compete.
    ps -eo pid=,stat=,comm=,args= 2>/dev/null \
        | awk '$2 !~ /^[ZXTt]/ && $3 ~ /^(xtask|cargo|rustc|clippy-driver|rustdoc)$/'
}

write_state() {
    loadavg=$(cat /proc/loadavg 2>/dev/null || echo 'unknown')
    nproc=$(nproc 2>/dev/null || echo unknown)
    qemu=$(list_qemu || true)
    jobs=$(list_jobs || true)
    {
        printf 'utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf 'context=%s\n' "$context"
        printf 'loadavg=%s\n' "$loadavg"
        printf 'nproc=%s\n' "$nproc"
        printf 'max_load=%s\n' "$max_load"
        printf 'wait_secs=%s\n' "$wait_secs"
        printf 'allow_jobs=%s\n' "$allow_jobs"
        printf 'allow_qemu=%s\n' "$allow_qemu"
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
    qemu=$(list_qemu || true)
    jobs=$(list_jobs || true)
    idle=1
    busy=    # colon-delimited reasons
    awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l <= m) }' || {
        idle=0
        busy="${busy}load($load1>$max_load) "
    }
    qemu_count=$(printf '%s\n' "$qemu" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')
    [ -n "$qemu_count" ] || qemu_count=0
    if [ "$qemu_count" -gt "$allow_qemu" ]; then
        idle=0
        busy="${busy}qemu($qemu_count>$allow_qemu) "
    fi
    if [ "$allow_jobs" -eq 1 ]; then
        [ -n "$jobs" ] && busy="${busy}jobs-ignored "
    else
        [ -n "$jobs" ] && { idle=0; busy="${busy}xtask/cargo/rustc "; }
    fi
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
