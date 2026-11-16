#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX archive runner: first-party ar, uuencode and uudecode host tests.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
present="ar uuencode uudecode"

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
            *) printf 'run-archive: unknown tool: %s\n' "$tool" >&2; exit 2 ;;
        esac
    done
fi

pass=0
fail=0
failed_names=
for tool in $selected; do
    case_dir=$here/archive/$tool
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$tool" "$case_dir"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi
    mkdir -p "$root/build/posix-utility/logs"
    case_log=$root/build/posix-utility/logs/archive-$tool.log
    set +e
    sh "$case_dir/test.sh" >"$case_log" 2>&1
    case_status=$?
    set -e
    if [ "$case_status" -eq 0 ]; then
        cases=$(grep -c '^ok: ' "$case_log" || true)
        printf '[PASS] %s: %s case(s)\n' "$tool" "$cases"
        pass=$((pass + 1))
    else
        printf '[FAIL] %s: exit %s\n' "$tool" "$case_status"
        sed 's/^/  /' "$case_log"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
    fi
done

printf 'archive-run: %s pass, %s fail\n' "$pass" "$fail"
if [ -n "$failed_names" ]; then
    printf 'archive-run: failed tools:%s\n' "$failed_names" >&2
    exit 1
fi
