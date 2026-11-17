#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Self-test for the required POSIX utility runner. The real tree is allowed to be red; this test
# checks that the runner reports the real state honestly and that a controlled fixture exercises
# PASS, FAIL and DEFECT without turning any of them into a pass.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/p4a8-required.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

failed=0
ok() {
    printf 'ok: %s\n' "$*"
}
not_ok() {
    printf 'not ok: %s\n' "$*"
    failed=$((failed + 1))
}

# Controlled fixture: time passes, times fails, the other five are missing their tests.
base=$tmp/root
for tool in time times tput ulimit localedef newgrp lp; do
    mkdir -p "$base/userland/bin/$tool"
done
mkdir -p "$base/tests/posix/utility/simple2/time" "$base/tests/posix/utility/simple2/times"
printf '#!/bin/sh\nexit 0\n' >"$base/tests/posix/utility/simple2/time/test.sh"
printf '#!/bin/sh\nexit 1\n' >"$base/tests/posix/utility/simple2/times/test.sh"

fixture=$tmp/fixture.out
set +e
REQUIRED_UTILITY_ROOT=$base sh "$here/runner.sh" >"$fixture" 2>&1
fixture_status=$?
set -e

[ "$fixture_status" -ne 0 ] || not_ok "fixture: runner exited 0 with failing/missing rows"
grep -q '^\[PASS\] time: tests/posix/utility/simple2/time/test.sh$' "$fixture" ||
    not_ok "fixture: time was not PASS"
grep -q '^\[FAIL\] times: exit 1 ' "$fixture" || not_ok "fixture: times was not FAIL"
grep -q '^\[DEFECT\] tput: missing .*tests/posix/utility/simple2/tput/test.sh$' "$fixture" ||
    not_ok "fixture: tput was not DEFECT"
grep -q '^required-runner: 1 pass, 1 fail, 5 defect$' "$fixture" ||
    not_ok "fixture: wrong summary"
fixture_rows=$(grep -Ec '^\[(PASS|FAIL|DEFECT)\] ' "$fixture" || true)
[ "$fixture_rows" -eq 7 ] || not_ok "fixture: expected 7 row lines, got $fixture_rows"
if [ "$failed" -eq 0 ]; then
    ok "fixture: PASS, FAIL and DEFECT states"
fi

# The real repository may be red; the runner must still print seven rows and use the exit status
# consistently with the summary.
real=$tmp/real.out
set +e
sh "$here/runner.sh" >"$real" 2>&1
real_status=$?
set -e

real_rows=$(grep -Ec '^\[(PASS|FAIL|DEFECT)\] ' "$real" || true)
[ "$real_rows" -eq 7 ] || not_ok "real: expected 7 row lines, got $real_rows"
summary=$(grep '^required-runner: ' "$real" | tail -1 || true)
printf '%s\n' "$summary" | grep -Eq '^required-runner: [0-9]+ pass, [0-9]+ fail, [0-9]+ defect$' ||
    not_ok "real: summary missing or malformed: $summary"
if printf '%s\n' "$summary" | grep -Eq '^required-runner: [0-9]+ pass, [0-9]+ fail, [0-9]+ defect$'; then
    counts=$(printf '%s\n' "$summary" | sed 's/^required-runner: //; s/ pass,//; s/ fail,//; s/ defect$//')
    # shellcheck disable=SC2086
    set -- $counts
    [ "$#" -eq 3 ] || not_ok "real: summary is not three counts"
    if [ "$#" -eq 3 ]; then
        [ $(( $1 + $2 + $3 )) -eq 7 ] || not_ok "real: counts do not sum to 7"
        if [ "$1" -eq 7 ]; then
            [ "$real_status" -eq 0 ] || not_ok "real: all rows pass but exit is $real_status"
        else
            [ "$real_status" -ne 0 ] || not_ok "real: non-pass rows but exit is 0"
        fi
    fi
fi
if [ "$failed" -eq 0 ]; then
    ok "real: seven rows and honest exit status"
fi

printf 'required-test: 2 cases, %s failed\n' "$failed"
[ "$failed" -eq 0 ]
