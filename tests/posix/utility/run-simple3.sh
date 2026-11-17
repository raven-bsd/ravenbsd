#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX simple3 utility runner: first-party localedef.
#
# Builds the host binary, then runs tests/posix/utility/simple3/<tool>/test.sh with UTIL pointing
# at it. The Raven target binary is host-only; the tests exercise the same parser/catalog code.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
target=$root/build/posix-utility/uutils-host
log=$root/build/posix-utility/build.log

present="localedef"

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
            *) printf 'run-simple3: unknown tool: %s\n' "$tool" >&2; exit 2 ;;
        esac
    done
fi

mkdir -p "$root/build/posix-utility"
printf '==> simple3 utilities: host build (%s)\n' "$target"
set +e
(
    cd "$root"
    build_args=""
    for tool in $selected; do
        build_args="$build_args -p $tool"
    done
    # shellcheck disable=SC2086
    CARGO_TARGET_DIR=$target cargo build --quiet --locked $build_args
) >"$log" 2>&1
build_status=$?
set -e
if [ "$build_status" -ne 0 ]; then
    tail -40 "$log" >&2
    printf 'run-simple3: host build failed (exit %s)\n' "$build_status" >&2
    exit 1
fi

pass=0
fail=0
failed_names=

for tool in $selected; do
    case_dir=$here/simple3/$tool
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$tool" "$case_dir"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi
    host_bin=$target/debug/$tool
    if [ ! -x "$host_bin" ]; then
        printf '[FAIL] %s: host build produced no binary\n' "$tool"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi

    mkdir -p "$root/build/posix-utility/logs"
    case_log=$root/build/posix-utility/logs/simple3-$tool.log
    set +e
    UTIL=$host_bin sh "$case_dir/test.sh" >"$case_log" 2>&1
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

printf 'simple3-run: %s pass, %s fail\n' "$pass" "$fail"
if [ -n "$failed_names" ]; then
    printf 'simple3-run: failed tools:%s\n' "$failed_names" >&2
    exit 1
fi
