#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P4f misc runner: the remaining vendored uutils tools that P4b/P4c/P4d do
# not own. Builds the host slice once, then runs one test.sh per tool in a
# fresh temporary directory with LC_ALL=C.
#
# A present tool with a failing case fails the run. This batch has all 40
# source tools; a source/package that disappears is reported as a defect.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
target=$root/build/posix-utility/misc-host
log=$root/build/posix-utility/misc-build.log

present="arch b2sum base32 base64 basenc dircolors factor false fmt groups hostid install logname md5sum mktemp numfmt nproc pinky printenv ptx seq sha1sum sha224sum sha256sum sha384sum sha512sum shred shuf sleep stdbuf sum sync tac timeout true truncate uptime users who yes"

if [ "${1:-}" = "--list" ]; then
    for tool in $present; do
        printf 'present\t%s\n' "$tool"
    done
    exit 0
fi

if [ "$#" -gt 0 ]; then
    selected=
    for tool in "$@"; do
        case " $present " in
            *" $tool "*) selected="$selected $tool" ;;
            *) printf 'run-misc: unknown tool: %s\n' "$tool" >&2; exit 2 ;;
        esac
    done
else
    selected=$present
fi

mkdir -p "$root/build/posix-utility"
printf '==> misc utilities: host build (%s)\n' "$target"
set +e
(
    cd "$root"
    build_args=""
    for tool in $selected; do
        build_args="$build_args -p uu_$tool"
    done
    # shellcheck disable=SC2086
    CARGO_TARGET_DIR=$target cargo build --quiet --locked $build_args \
        --manifest-path userland/uutils/Cargo.toml
) >"$log" 2>&1
build_status=$?
set -e
if [ "$build_status" -ne 0 ]; then
    tail -40 "$log" >&2
    printf 'run-misc: host build failed (exit %s)\n' "$build_status" >&2
    exit 1
fi

pass=0
fail=0
failed_names=

for tool in $selected; do
    case_dir=$here/misc/$tool
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
    case_log=$root/build/posix-utility/logs/misc-$tool.log
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

printf 'misc-run: %s pass, %s fail\n' "$pass" "$fail"
if [ -n "$failed_names" ]; then
    printf 'misc-run: failed tools:%s\n' "$failed_names" >&2
    exit 1
fi
