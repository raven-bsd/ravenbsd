#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX lex/yacc behaviour runner. Builds the two first-party generators, then runs the per-tool
# fixtures under tests/posix/utility/parsers/<tool>/test.sh.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
target=$root/build/posix-utility/parsers
log=$root/build/posix-utility/parsers-build.log

present="lex yacc"

if [ "${1:-}" = "--list" ]; then
    for tool in $present; do
        printf 'present\t%s\n' "$tool"
    done
    exit 0
fi

mkdir -p "$root/build/posix-utility"
printf '==> parser generators: host build (%s)\n' "$target"
set +e
(
    cd "$root"
    CARGO_TARGET_DIR=$target cargo build --quiet --locked -p lex -p yacc
) >"$log" 2>&1
build_status=$?
set -e
if [ "$build_status" -ne 0 ]; then
    tail -40 "$log" >&2
    printf 'run-parsers: host build failed (exit %s)\n' "$build_status" >&2
    exit 1
fi

pass=0
fail=0
failed_names=

for tool in $present; do
    case_dir=$here/parsers/$tool
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$tool" "$case_dir"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi
    mkdir -p "$root/build/posix-utility/logs"
    case_log=$root/build/posix-utility/logs/parsers-$tool.log
    set +e
    LEX=$target/debug/lex YACC=$target/debug/yacc sh "$case_dir/test.sh" >"$case_log" 2>&1
    case_status=$?
    set -e
    if [ "$case_status" -eq 0 ]; then
        printf '[PASS] %s\n' "$tool"
        sed 's/^/  /' "$case_log"
        pass=$((pass + 1))
    else
        printf '[FAIL] %s: exit %s\n' "$tool" "$case_status"
        sed 's/^/  /' "$case_log"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
    fi
done

printf 'parsers-run: %s pass, %s fail, 0 defect\n' "$pass" "$fail"
if [ -n "$failed_names" ]; then
    printf 'parsers-run: failed tools:%s\n' "$failed_names" >&2
    exit 1
fi
