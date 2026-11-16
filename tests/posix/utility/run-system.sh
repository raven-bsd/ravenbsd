#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P4d system/process utility runner. Runs the QEMU shell case for the present
# tools and prints the matrix; missing tools stay DEFECT and are never claimed
# as present. Writes only under build/posix/utility/system.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

matrix=tests/posix/utility/system/cases.tsv
out=build/posix/utility/system
mkdir -p "$out"

[ -f "$matrix" ] || {
    echo "posix-util-system: matrix missing: $matrix" >&2
    exit 1
}

if cargo xtask qtest --case posix-util-system --arch x86_64 >"$out/qtest.log" 2>&1; then
    qtest_code=0
    qtest_status=PASS
else
    qtest_code=$?
    qtest_status=FAIL
fi

echo "posix-util-system: qtest $qtest_status (log $out/qtest.log)"
printf 'row\ttool\tstatus\tcommand\tdetail\n'
while IFS="$(printf '\t')" read -r row tool command defect; do
    case "$row" in
        ''|'#'*) continue ;;
    esac
    if [ -n "$defect" ]; then
        status=DEFECT
        detail=$defect
    elif [ -f "tests/posix/utility/system/$tool/test.sh" ]; then
        if sh "tests/posix/utility/system/$tool/test.sh" </dev/null >"$out/$tool-test.log" 2>&1; then
            status=PASS
            detail="host test"
        else
            status=FAIL
            detail="host test failed (log $out/$tool-test.log)"
        fi
    elif [ "$qtest_status" = PASS ]; then
        status=PASS
        detail=qtest
    else
        status=FAIL
        detail="qtest failed"
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$row" "$tool" "$status" "$command" "$detail"
done <"$matrix"

exit "$qtest_code"
