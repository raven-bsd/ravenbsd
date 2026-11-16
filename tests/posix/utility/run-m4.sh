#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX m4 behaviour runner: builds the first-party host binary and runs the
# per-feature cases under tests/posix/utility/m4/<feature>/test.sh.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
target=$root/build/posix-utility/m4-host
log=$root/build/posix-utility/build-m4.log

present="define quoting conditionals arithmetic strings diversions include cli errors"

if [ "${1:-}" = "--list" ]; then
    for feature in $present; do
        printf 'present\t%s\n' "$feature"
    done
    exit 0
fi

selected="$present"
if [ "$#" -gt 0 ]; then
    selected=
    for feature in "$@"; do
        case " $present " in
            *" $feature "*) selected="$selected $feature" ;;
            *) printf 'run-m4: unknown feature: %s\n' "$feature" >&2; exit 2 ;;
        esac
    done
fi

mkdir -p "$root/build/posix-utility"
printf '==> m4: host build (%s)\n' "$target"
set +e
(
    cd "$root"
    CARGO_TARGET_DIR=$target cargo build --quiet --locked -p raven-m4
) >"$log" 2>&1
build_status=$?
set -e
if [ "$build_status" -ne 0 ]; then
    tail -40 "$log" >&2
    printf 'run-m4: host build failed (exit %s)\n' "$build_status" >&2
    exit 1
fi

host_bin=$target/debug/raven-m4
if [ ! -x "$host_bin" ]; then
    printf 'run-m4: host build produced no binary\n' >&2
    exit 1
fi

pass=0
fail=0
failed_names=

for feature in $selected; do
    case_dir=$here/m4/$feature
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$feature" "$case_dir"
        fail=$((fail + 1))
        failed_names="$failed_names $feature"
        continue
    fi

    mkdir -p "$root/build/posix-utility/logs"
    case_log=$root/build/posix-utility/logs/m4-$feature.log
    set +e
    UTIL=$host_bin sh "$case_dir/test.sh" >"$case_log" 2>&1
    case_status=$?
    set -e
    if [ "$case_status" -eq 0 ]; then
        cases=$(grep -c '^ok: ' "$case_log" || true)
        printf '[PASS] %s: %s case(s)\n' "$feature" "$cases"
        pass=$((pass + 1))
    else
        printf '[FAIL] %s: exit %s\n' "$feature" "$case_status"
        sed 's/^/  /' "$case_log"
        fail=$((fail + 1))
        failed_names="$failed_names $feature"
    fi
done

printf 'm4-run: %s pass, %s fail\n' "$pass" "$fail"
if [ -n "$failed_names" ]; then
    printf 'm4-run: failed features:%s\n' "$failed_names" >&2
    exit 1
fi
