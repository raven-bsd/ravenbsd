#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host preflight for the clean 24 h fault-injection soak (C74gy).
#
# The first 24 h soak was invalid because it ran on a shared host under concurrent load.
# This checker refuses a busy host before the run: QEMU, a fault-injection soak, another
# minion's cargo/rustc build, low disk/memory, a dirty worktree, a failed patched
# fault-injection self-test, an existing log/sidecar path or an unusable timeout.
#
# It never starts QEMU or the soak. `--check` only probes the host; `--self-test` runs the
# synthetic cases under tests/release/soak-preflight/.
#
# Synthetic test hooks (all unset in a real run):
#   SOAK_PREFLIGHT_QEMU_LIST / SOAK_PREFLIGHT_BUILD_LIST   fake process lists
#   SOAK_PREFLIGHT_DISK_AVAIL_KB / SOAK_PREFLIGHT_MEM_AVAIL_KB
#   SOAK_PREFLIGHT_LOAD1 / SOAK_PREFLIGHT_NPROC
#   SOAK_PREFLIGHT_GIT_DIRTY=1
#   SOAK_PREFLIGHT_FAULT_SELF_TEST=<shell command>
#   SOAK_PREFLIGHT_LOG_DIR / SOAK_PREFLIGHT_STAMP

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

soak_seconds=86400
timeout_seconds=90000
min_disk_kb=${SOAK_PREFLIGHT_MIN_DISK_KB:-10485760}
min_mem_kb=${SOAK_PREFLIGHT_MIN_MEM_KB:-2097152}
log_dir=${SOAK_PREFLIGHT_LOG_DIR:-build/soak}
stamp=${SOAK_PREFLIGHT_STAMP:-$(date -u +%Y%m%dT%H%M%SZ)}
log=$log_dir/soak-$stamp.log
sidecar=$log_dir/soak-$stamp.json

usage() {
    cat <<'EOF'
usage: sh scripts/soak-preflight.sh --check
       sh scripts/soak-preflight.sh --self-test
       sh scripts/soak-preflight.sh --help

--check      verify the host is idle enough for the clean 24 h soak; never starts QEMU.
--self-test  run the synthetic idle/busy/disk/self-test cases.
EOF
}

refuse() {
    printf 'soak-preflight: REFUSED: %s\n' "$*" >&2
    exit 1
}

is_uint() {
    case $1 in
        ''|*[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

process_matches() {
    list=$1
    pattern=$2
    if [ -n "$list" ]; then
        cat "$list"
    else
        pgrep -af "$pattern" 2>/dev/null |
            grep -v 'soak-preflight.sh' |
            grep -v "^$$ " |
            grep -v "^$PPID " || true
    fi
}

case ${1:-} in
    --self-test)
        exec sh tests/release/soak-preflight/self-test.sh
        ;;
    --help|-h)
        usage
        exit 0
        ;;
    --check)
        shift
        [ "$#" -eq 0 ] || { usage >&2; exit 2; }
        ;;
    '')
        usage >&2
        exit 2
        ;;
    *)
        printf 'soak-preflight: unknown argument %s\n' "$1" >&2
        usage >&2
        exit 2
        ;;
esac

# 1. No QEMU, no other soak, no concurrent cargo/rustc build.
qemu=$(process_matches "${SOAK_PREFLIGHT_QEMU_LIST:-}" 'qemu-system-')
if [ -n "$qemu" ]; then
    refuse "busy: QEMU process: $(printf '%s\n' "$qemu" | head -n 1)"
fi
build=$(process_matches "${SOAK_PREFLIGHT_BUILD_LIST:-}" 'fault-injection\.sh|cargo|rustc')
if [ -n "$build" ]; then
    refuse "busy: build/qtest/soak process: $(printf '%s\n' "$build" | head -n 1)"
fi

# 2. Load average must not exceed the CPU count.
load1=${SOAK_PREFLIGHT_LOAD1:-$(awk '{print $1}' /proc/loadavg)}
nproc=${SOAK_PREFLIGHT_NPROC:-$(nproc)}
is_uint "$nproc" && [ "$nproc" -gt 0 ] || refuse "invalid CPU count: $nproc"
if awk -v l="$load1" -v cpus="$nproc" 'BEGIN { exit !(l > cpus) }'; then
    refuse "busy: load average $load1 exceeds $nproc CPUs"
fi

# 3. Disk and memory headroom.
disk_avail=${SOAK_PREFLIGHT_DISK_AVAIL_KB:-$(df -Pk "$root" | awk 'NR == 2 { print $4 }')}
is_uint "$disk_avail" || refuse "cannot read free disk space"
if [ "$disk_avail" -lt "$min_disk_kb" ]; then
    refuse "low disk: ${disk_avail} KiB free, need ${min_disk_kb} KiB"
fi
mem_avail=${SOAK_PREFLIGHT_MEM_AVAIL_KB:-$(awk '/MemAvailable:/ { print $2 }' /proc/meminfo)}
is_uint "$mem_avail" || refuse "cannot read available memory"
if [ "$mem_avail" -lt "$min_mem_kb" ]; then
    refuse "low memory: ${mem_avail} KiB available, need ${min_mem_kb} KiB"
fi

# 4. The patched fault-injection driver must self-test.
if [ -n "${SOAK_PREFLIGHT_FAULT_SELF_TEST:-}" ]; then
    if ! fault_out=$(sh -c "$SOAK_PREFLIGHT_FAULT_SELF_TEST" 2>&1); then
        refuse "fault-injection self-test failed: $fault_out"
    fi
else
    if ! fault_out=$(sh scripts/fault-injection.sh --self-test 2>&1); then
        refuse "fault-injection self-test failed: $fault_out"
    fi
fi
printf '%s\n' "$fault_out" | grep -q 'PASS' ||
    refuse "fault-injection self-test did not report PASS: $fault_out"

# 5. Clean worktree and commit stamp. The 0/1 overrides are synthetic-test hooks; a real
# run leaves SOAK_PREFLIGHT_GIT_DIRTY unset and reads the worktree.
case "${SOAK_PREFLIGHT_GIT_DIRTY:-}" in
    1)
        refuse "worktree is dirty; commit or stash before the soak"
        ;;
    0)
        ;;
    *)
        dirty=$(git status --porcelain)
        if [ -n "$dirty" ]; then
            refuse "worktree is dirty: $(printf '%s\n' "$dirty" | head -n 1)"
        fi
        ;;
esac
commit=$(git rev-parse HEAD)
branch=$(git rev-parse --abbrev-ref HEAD)

# 6. Fresh log and sidecar paths under the chosen directory.
case $stamp in
    */*|..*) refuse "invalid log stamp: $stamp" ;;
esac
if ! mkdir -p "$log_dir"; then
    refuse "cannot create log directory: $log_dir"
fi
if [ -e "$log" ] || [ -e "$sidecar" ]; then
    refuse "log/sidecar path already exists: $log or $sidecar"
fi
probe=$log_dir/.soak-preflight-probe-$$
if ! : > "$probe"; then
    refuse "log directory is not writable: $log_dir"
fi
rm -f "$probe"

# 7. A 24 h soak needs a timeout longer than the budget.
if ! command -v timeout >/dev/null 2>&1; then
    refuse "timeout(1) not found"
fi
if ! timeout 1 true >/dev/null 2>&1; then
    refuse "timeout(1) is not usable"
fi
if [ "$timeout_seconds" -le "$soak_seconds" ]; then
    refuse "timeout ${timeout_seconds}s is not longer than the ${soak_seconds}s soak"
fi

preflight_json=$log_dir/preflight-$stamp-$$.json
command_text="timeout $timeout_seconds sh scripts/fault-injection.sh --profile release --arch x86_64 --soak $soak_seconds > $log 2>&1"
{
    printf '{\n'
    printf '  "commit": "%s",\n' "$commit"
    printf '  "branch": "%s",\n' "$branch"
    printf '  "utc": "%s",\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '  "soak_seconds": %s,\n' "$soak_seconds"
    printf '  "timeout_seconds": %s,\n' "$timeout_seconds"
    printf '  "log": "%s",\n' "$log"
    printf '  "sidecar": "%s",\n' "$sidecar"
    printf '  "command": "%s"\n' "$command_text"
    printf '}\n'
} > "$preflight_json"

printf 'soak-preflight: READY\n'
printf '  commit=%s branch=%s\n' "$commit" "$branch"
printf '  disk_avail_kb=%s memory_avail_kb=%s load1=%s nproc=%s\n' \
    "$disk_avail" "$mem_avail" "$load1" "$nproc"
printf '  fault-injection self-test: PASS\n'
printf '  log=%s\n' "$log"
printf '  sidecar=%s\n' "$sidecar"
printf '  preflight=%s\n' "$preflight_json"
printf '  command: %s\n' "$command_text"
