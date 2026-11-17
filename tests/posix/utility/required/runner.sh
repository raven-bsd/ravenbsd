#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Required POSIX utility acceptance runner. It runs the per-tool host test for the seven utility
# rows that P4a7 left required and prints one PASS/FAIL/DEFECT line per row plus a summary. A
# missing tool test or tool source is DEFECT, a non-zero test is FAIL; the runner never turns a
# missing row into a pass. Exit status is 0 only when all seven rows pass.
#
# REQUIRED_UTILITY_ROOT is a test-only override used by test.sh to exercise the three states with
# a controlled fixture tree. Normal acceptance runs use the repository root.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
default_root=$(CDPATH= cd -- "$here/../../../.." && pwd)
root=${REQUIRED_UTILITY_ROOT:-$default_root}
logs_rel=build/posix-utility/logs

tools="localedef lp newgrp time times tput ulimit"

test_path() {
    case "$1" in
        time|times|tput|ulimit)
            printf 'tests/posix/utility/simple2/%s/test.sh' "$1"
            ;;
        localedef)
            printf 'tests/posix/utility/simple3/localedef/test.sh'
            ;;
        newgrp)
            printf 'tests/posix/utility/simple4/newgrp/test.sh'
            ;;
        lp)
            printf 'tests/posix/utility/simple5/lp/test.sh'
            ;;
    esac
}

pass=0
fail=0
defect=0

for tool in $tools; do
    tp=$(test_path "$tool")
    case_test=$root/$tp
    tool_dir=$root/userland/bin/$tool
    if [ ! -d "$tool_dir" ] || [ ! -f "$case_test" ]; then
        missing=
        [ ! -d "$tool_dir" ] && missing="$missing userland/bin/$tool"
        [ ! -f "$case_test" ] && missing="$missing $tp"
        printf '[DEFECT] %s: missing%s\n' "$tool" "$missing"
        defect=$((defect + 1))
        continue
    fi
    mkdir -p "$root/$logs_rel"
    log=$root/$logs_rel/required-$tool.log
    set +e
    sh "$case_test" >"$log" 2>&1
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        printf '[PASS] %s: %s\n' "$tool" "$tp"
        pass=$((pass + 1))
    else
        printf '[FAIL] %s: exit %s (log: %s)\n' "$tool" "$status" "$logs_rel/required-$tool.log"
        sed 's/^/  /' "$log"
        fail=$((fail + 1))
    fi
done

printf 'required-runner: %s pass, %s fail, %s defect\n' "$pass" "$fail" "$defect"
[ "$fail" -eq 0 ] && [ "$defect" -eq 0 ]
