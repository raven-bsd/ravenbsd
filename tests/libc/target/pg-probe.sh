#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Mechanical PG probe for the on-target libc cases (C73 4.2). It is read-only, prints one
# `PG-n: present|absent|unknown` line per probed item and one RUNNABLE/BLOCKED line per case,
# and always exits 0. `RL_PROBE_ROOT` and `RL_PROBE_TSV` exist for the host fixture tests; the
# real run uses the repository root and tests/libc/target/cases.tsv.

set -eu

script_root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
root=${RL_PROBE_ROOT:-$script_root}
tsv=${RL_PROBE_TSV:-$root/tests/libc/target/cases.tsv}

present() { printf '%s\n' present; }
absent() { printf '%s\n' absent; }
unknown() { printf '%s\n' unknown; }

probe() {
    pg=$1
    case $pg in
        1)
            if grep -qs 'THREAD_SET_TLS' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        2)
            if grep -qs 'THREAD_SELF' "$root/crates/raven-abi/src/syscall.rs" &&
                grep -qs 'THREAD_EXIT_EX' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        3)
            if grep -Rqs 'CMP_REQUEUE' "$root/crates/raven-abi/src"; then
                present
            else
                absent
            fi
            ;;
        7)
            if grep -qs 'SIG_SETUP' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        8)
            if grep -qs 'THREAD_KICK' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        12)
            if grep -Eisq 'fxsave|xsave' "$root/kernel/src/sched/context.rs"; then
                present
            else
                absent
            fi
            ;;
        13)
            if grep -Rqs 'REPLACE' "$root/crates/raven-abi/src"; then
                present
            else
                absent
            fi
            ;;
        17)
            if grep -qs 'CLOCK_NANOSLEEP' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        22)
            if grep -Eqs 'PREAD|SEEK' "$root/crates/raven-abi/src/syscall.rs" ||
                grep -Rqs 'Seek' "$root/crates/raven-scheme/src"; then
                present
            else
                absent
            fi
            ;;
        23)
            if [ ! -f "$root/kernel/src/syscall/ipc.rs" ]; then
                unknown
            else
                count=$(grep -c 'file_from_handle(caller, handle, Rights::NONE)' \
                    "$root/kernel/src/syscall/ipc.rs" 2>/dev/null || true)
                if [ "${count:-0}" -ge 2 ]; then
                    present
                else
                    absent
                fi
            fi
            ;;
        24)
            if grep -RqsE 'create_mode|CREATE_MODE' "$root/crates/raven-proto-fs/src"; then
                present
            else
                absent
            fi
            ;;
        25)
            if grep -Rqs 'Notify' "$root/crates/raven-scheme/src"; then
                present
            else
                absent
            fi
            ;;
        29)
            if grep -qs 'ENOTSUP' "$root/crates/raven-abi/src/errno.rs" &&
                grep -qs 'EDEADLK' "$root/crates/raven-abi/src/errno.rs"; then
                present
            else
                absent
            fi
            ;;
        35)
            if [ -n "$(find "$root/userland/servers" -maxdepth 1 -name 'pipe*' -print -quit 2>/dev/null)" ]; then
                present
            else
                absent
            fi
            ;;
        44)
            if [ -d "$root/userland/servers/procd" ]; then
                present
            else
                absent
            fi
            ;;
        45)
            # PROC_FORK is a kernel primitive (S50b); the ABI crate has no variant yet.
            if grep -qs 'PROC_FORK' "$root/crates/raven-abi/src/syscall.rs" ||
                grep -qs 'pub const PROC_FORK' "$root/kernel/src/syscall/process_ctl.rs"; then
                present
            else
                absent
            fi
            ;;
        47)
            if grep -qs 'HANDLE_COPY' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        49)
            if [ ! -f "$root/userland/rt/src/spawn.rs" ]; then
                unknown
            elif grep -qs 'HasInterpreter' "$root/userland/rt/src/spawn.rs"; then
                absent
            else
                present
            fi
            ;;
        50)
            if grep -qs 'AT_BASE' "$root/userland/rt/src/spawn.rs"; then
                present
            else
                absent
            fi
            ;;
        52)
            if grep -qs 'GETRANDOM' "$root/crates/raven-abi/src/syscall.rs"; then
                present
            else
                absent
            fi
            ;;
        53)
            if grep -Rqs 'max-page-size' "$root/xtask/src" 2>/dev/null ||
                grep -qs 'max-page-size' "$root/.cargo/config.toml" 2>/dev/null ||
                grep -Rqs 'max-page-size' "$root/target-specs" 2>/dev/null; then
                present
            else
                absent
            fi
            ;;
        *)
            unknown
            ;;
    esac
}

ALL_PGS='1 2 3 7 8 12 13 17 22 23 24 25 29 35 44 45 47 49 50 52 53'

row_for() {
    awk -F '	' -v want="$1" 'BEGIN { found = 0 }
        $0 ~ /^#/ || $0 == "" || $1 == "case" { next }
        $1 == want { print; found = 1; exit }
        END { if (found == 0) exit 1 }' "$tsv"
}

case_line() {
    case_name=$1
    row=$(row_for "$case_name") || {
        printf 'case %s: BLOCKED needs PG-? (no cases.tsv row)\n' "$case_name"
        return 0
    }
    hard=$(printf '%s\n' "$row" | cut -f2)
    missing=''
    for pg in $hard; do
        state=$(probe "$pg")
        if [ "$state" != present ]; then
            missing="$missing PG-$pg"
        fi
    done
    if [ -z "$missing" ]; then
        printf 'case %s: RUNNABLE\n' "$case_name"
    else
        printf 'case %s: BLOCKED needs%s\n' "$case_name" "$missing"
    fi
}

print_case_pgs() {
    case_name=$1
    row=$(row_for "$case_name") || return 1
    pgs=$(printf '%s\n' "$row" | cut -f2,3 | tr '\t' ' ' | tr ' ' '\n' | sed '/^$/d' | sort -n | uniq)
    for pg in $pgs; do
        printf 'PG-%s: %s\n' "$pg" "$(probe "$pg")"
    done
    case_line "$case_name"
}

if [ ! -f "$tsv" ]; then
    printf 'pg-probe: cases.tsv not found: %s\n' "$tsv" >&2
    exit 0
fi

if [ "$#" -eq 0 ] || [ "$1" = all ]; then
    for pg in $ALL_PGS; do
        printf 'PG-%s: %s\n' "$pg" "$(probe "$pg")"
    done
    while IFS='	' read -r name rest; do
        case $name in
            '' | '#'* | case) continue ;;
        esac
        case_line "$name"
    done < "$tsv"
else
    name=${1#libc-}
    print_case_pgs "$name"
fi
