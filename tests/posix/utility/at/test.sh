#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# at/batch host cases: schedule forms, list, remove, validation and the idle queue.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo build -q -p at -p batch
at_bin="$root/target/debug/at"
batch_bin="$root/target/debug/batch"
[ -x "$at_bin" ] || {
    echo "at test: missing $at_bin" >&2
    exit 1
}
[ -x "$batch_bin" ] || {
    echo "at test: missing $batch_bin" >&2
    exit 1
}

work="$root/build/posix/utility/at"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

export AT_SPOOL="$work/spool"

fail() {
    echo "at test: FAIL $*" >&2
    exit 1
}

list="$work/list"

"$at_bin" -l >"$list" || fail "empty list"
if [ -s "$list" ]; then
    fail "empty queue listed jobs"
fi

out=$(printf 'echo at-hello\n' | "$at_bin" now + 5 minutes) || fail "schedule stdin"
printf '%s\n' "$out" | grep -q '^at: scheduled ' || fail "schedule output: $out"
"$at_bin" -l >"$list" || fail "list after schedule"
grep -q 'echo at-hello' "$list" || fail "list hello"
id=$(awk '/echo at-hello/ {print $1; exit}' "$list")
[ -n "$id" ] || fail "job id"

"$at_bin" 23:59 echo hhmm >/dev/null || fail "schedule HH:MM"
"$at_bin" now + 1 minutes echo relative >/dev/null || fail "schedule relative words"
"$at_bin" 2030-01-01 00:00 echo dated >/dev/null || fail "schedule date"
printf 'echo at-file\n' >"$work/command"
"$at_bin" -f "$work/command" now + 2 minutes >/dev/null || fail "schedule -f"
printf 'echo batch-hello\n' | "$batch_bin" >/dev/null || fail "batch stdin"
"$at_bin" -l >"$list" || fail "list all"
grep -q 'echo hhmm' "$list" || fail "list HH:MM"
grep -q 'echo relative' "$list" || fail "list relative"
grep -q 'echo dated' "$list" || fail "list date"
grep -q 'echo at-file' "$list" || fail "list -f"
grep -q 'echo batch-hello' "$list" || fail "list batch"

if "$at_bin" not-a-time echo bad >/dev/null 2>&1; then
    fail "invalid time accepted"
fi
if "$at_bin" 2020-01-01 00:00 echo past >/dev/null 2>&1; then
    fail "past time accepted"
fi
if printf '\n' | "$at_bin" now + 1 minutes >/dev/null 2>&1; then
    fail "empty command accepted"
fi
if "$at_bin" now + 1 minutes echo 'a > b' >/dev/null 2>&1; then
    fail "shell metacharacters accepted"
fi
if "$at_bin" -l -r >/dev/null 2>&1; then
    fail "-l -r accepted"
fi
if "$at_bin" -r >/dev/null 2>&1; then
    fail "-r without id accepted"
fi

"$at_bin" -r "$id" || fail "remove"
"$at_bin" -l >"$list" || fail "list after remove"
if grep -q 'echo at-hello' "$list"; then
    fail "removed job still listed"
fi
if "$at_bin" -r "$id" >/dev/null 2>&1; then
    fail "removing a missing job succeeded"
fi
if "$at_bin" -r 9999999999.1 >/dev/null 2>&1; then
    fail "removing an unknown job succeeded"
fi

files=$(find "$AT_SPOOL" -name '*.job' -type f | wc -l)
[ "$files" -ge 1 ] || fail "spool has no job files"

echo "at: ok"
