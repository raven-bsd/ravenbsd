#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX simple-utility runner (P4g): time, times, tput, tabs, ulimit, unget and what.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
present="time times tput tabs ulimit unget what"

if [ "${1:-}" = "--list" ]; then
    for tool in $present; do
        printf 'present\t%s\n' "$tool"
    done
    exit 0
fi

selected="$present"
if [ "$#" -gt 0 ]; then
    selected=
    for tool in "$@"; do
        case " $present " in
            *" $tool "*) selected="$selected $tool" ;;
            *) printf 'run-simple2: unknown tool: %s\n' "$tool" >&2; exit 2 ;;
        esac
    done
fi

pass=0
fail=0
for tool in $selected; do
    case_dir=$here/simple2/$tool
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$tool" "$case_dir"
        fail=$((fail + 1))
        continue
    fi
    mkdir -p "$root/build/posix-utility/logs"
    log=$root/build/posix-utility/logs/simple2-$tool.log
    set +e
    sh "$case_dir/test.sh" >"$log" 2>&1
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        cases=$(grep -c '^ok: ' "$log" || true)
        printf '[PASS] %s: %s case(s)\n' "$tool" "$cases"
        pass=$((pass + 1))
    else
        printf '[FAIL] %s: exit %s\n' "$tool" "$status"
        sed 's/^/  /' "$log"
        fail=$((fail + 1))
    fi
done
printf 'simple2-run: %s pass, %s fail\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
