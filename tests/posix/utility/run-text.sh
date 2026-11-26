#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX text utility behaviour runner.
#
# Builds the vendored uutils text utilities for the host once, then runs
# tests/posix/utility/text/<tool>/test.sh for every tool whose source exists.
# The Raven target binaries need the Raven kernel, so the host build exercises
# the same utility source; the runner is the ticket's executable evidence.
#
# A present tool with a failing case fails the run. The awk, sed, grep, cmp,
# diff and strings rows are first-party and covered by run-text2.sh; they are
# not defects of this uutils text batch.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
target=$root/build/posix-utility/uutils-host
log=$root/build/posix-utility/build.log

present="sort cut tr wc head tail uniq comm fold expand unexpand paste join nl pr od split csplit tsort tee printf basename dirname expr"
missing=""

if [ "${1:-}" = "--list" ]; then
    for tool in $present; do
        printf 'present\t%s\n' "$tool"
    done
    for tool in $missing; do
        printf 'defect\t%s\n' "$tool"
    done
    exit 0
fi

if [ "$#" -gt 0 ]; then
    selected=
    for tool in "$@"; do
        case " $present $missing " in
            *" $tool "*) selected="$selected $tool" ;;
            *) printf 'run-text: unknown tool: %s\n' "$tool" >&2; exit 2 ;;
        esac
    done
else
    selected="$present $missing"
fi

mkdir -p "$root/build/posix-utility"
printf '==> text utilities: host build (%s)\n' "$target"
set +e
(
    cd "$root"
    build_args=""
    for tool in $selected; do
        case " $present " in
            *" $tool "*) build_args="$build_args -p uu_$tool" ;;
        esac
    done
    if [ -n "$build_args" ]; then
        # shellcheck disable=SC2086
        CARGO_TARGET_DIR=$target cargo build --quiet --locked $build_args \
            --manifest-path userland/uutils/Cargo.toml
    fi
) >"$log" 2>&1
build_status=$?
set -e
if [ "$build_status" -ne 0 ]; then
    tail -40 "$log" >&2
    printf 'run-text: host build failed (exit %s)\n' "$build_status" >&2
    exit 1
fi

pass=0
fail=0
defect=0
failed_names=
defect_names=

for tool in $selected; do
    case " $missing " in
        *" $tool "*)
            printf '[DEFECT] %s: no text utility implementation; partial (see P4-text.md)\n' "$tool"
            defect=$((defect + 1))
            defect_names="$defect_names $tool"
            continue
            ;;
    esac

    case_dir=$here/text/$tool
    if [ ! -f "$case_dir/test.sh" ]; then
        printf '[FAIL] %s: missing %s/test.sh\n' "$tool" "$case_dir"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi
    if [ ! -x "$target/debug/$tool" ]; then
        printf '[FAIL] %s: host build produced no binary\n' "$tool"
        fail=$((fail + 1))
        failed_names="$failed_names $tool"
        continue
    fi

    mkdir -p "$root/build/posix-utility/logs"
    case_log=$root/build/posix-utility/logs/$tool.log
    set +e
    UTIL=$target/debug/$tool sh "$case_dir/test.sh" >"$case_log" 2>&1
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

printf 'text-run: %s pass, %s fail, %s defect\n' "$pass" "$fail" "$defect"
if [ -n "$defect_names" ]; then
    printf 'text-run: partial tools:%s\n' "$defect_names"
fi
if [ -n "$failed_names" ]; then
    printf 'text-run: failed tools:%s\n' "$failed_names" >&2
    exit 1
fi
