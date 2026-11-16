#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `xargs` utility.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
target=$root/build/posix-utility/xargs-host
if [ -z "${UTIL:-}" ]; then
    printf '==> xargs: host build (%s)\n' "$target"
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p raven-xargs
    ) || {
        printf 'xargs test: host build failed\n' >&2
        exit 1
    }
    UTIL=$target/debug/raven-xargs
fi
if [ ! -x "$UTIL" ]; then
    printf 'xargs test: missing host binary\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

check_file() {
    name=$1
    expected=$2
    actual=$3
    if ! cmp -s "$expected" "$actual"; then
        printf 'FAIL: %s\n' "$name" >&2
        printf '%s\n' '--- expected ---' >&2
        cat "$expected" >&2
        printf '%s\n' '--- actual ---' >&2
        cat "$actual" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

cd "$tmp"
printf 'a b\nc\n' >input
printf 'a b c\n' >expected
"$UTIL" echo <input >out 2>err
check_file 'appends whitespace items' expected out

printf 'a b c d e\n' >input
printf 'a b\nc d\ne\n' >expected
"$UTIL" -n 2 echo <input >out 2>err
check_file '-n batches arguments' expected out

printf 'a\nb\n' >input
printf '[a]\n[b]\n' >expected
"$UTIL" -I {} echo '[{}]' <input >out 2>err
check_file '-I replaces the marker' expected out

printf 'a\nb\n' >input
printf 'hi\nhi\n' >expected
"$UTIL" -I {} echo hi <input >out 2>err
check_file '-I runs once per item without a marker' expected out

printf 'a b\0c\0' >input
printf 'a b c\n' >expected
"$UTIL" -0 echo <input >out 2>err
check_file '-0 splits on NUL' expected out

printf 'a b\0c\0' >input
printf '<a b>\n<c>\n' >expected
"$UTIL" -0 -I {} echo '<{}>' <input >out 2>err
check_file '-0 and -I preserve spaces' expected out

printf '' >input
"$UTIL" -r echo <input >out 2>err
if [ -s out ]; then
    fail '-r with empty input produces no output'
fi
printf 'ok: -r with empty input produces no output\n'

printf '\n' >expected
"$UTIL" echo <input >out 2>err
check_file 'empty input runs the command once' expected out

printf 'a\nb\n' >input
"$UTIL" -n 1 -t echo <input >out 2>err
printf 'a\nb\n' >expected
check_file '-t still runs the command' expected out
if ! grep -q 'echo a' err || ! grep -q 'echo b' err; then
    fail '-t traces each command'
fi
printf 'ok: -t traces each command\n'

printf 'aaa bbb\n' >input
printf 'aaa\nbbb\n' >expected
"$UTIL" -s 12 echo <input >out 2>err
check_file '-s splits oversized batches' expected out

printf 'overflow\n' >input
set +e
"$UTIL" -x -s 8 echo <input >out 2>err
status=$?
set -e
if [ "$status" -eq 0 ] || [ -s out ]; then
    fail '-x rejects an oversized item'
fi
printf 'ok: -x rejects an oversized item\n'

printf 'aaa bbb ccc\n' >input
printf 'aaa\nbbb\nccc\n' >expected
"$UTIL" -n 3 -s 12 echo <input >out 2>err
check_file '-n splits when a group exceeds -s' expected out

printf 'aaa bbb ccc\n' >input
set +e
"$UTIL" -x -n 3 -s 12 echo <input >out 2>err
status=$?
set -e
if [ "$status" -eq 0 ] || [ -s out ]; then
    fail '-x rejects an -n group that exceeds -s'
fi
printf 'ok: -x rejects an -n group that exceeds -s\n'

printf 'x\n' >input
set +e
"$UTIL" false <input >out 2>err
status=$?
set -e
if [ "$status" -ne 123 ]; then
    fail 'child failure maps to status 123'
fi
printf 'ok: child failure maps to status 123\n'

printf 'x\n' >input
set +e
"$UTIL" sh -c 'exit 255' <input >out 2>err
status=$?
set -e
if [ "$status" -ne 124 ]; then
    fail 'child status 255 maps to status 124'
fi
printf 'ok: child status 255 maps to status 124\n'

printf 'x\n' >input
set +e
"$UTIL" sh -c 'kill -TERM $$' <input >out 2>err
status=$?
set -e
if [ "$status" -ne 125 ]; then
    fail 'signal termination maps to status 125'
fi
printf 'ok: signal termination maps to status 125\n'

printf 'x\n' >input
set +e
"$UTIL" definitely-not-a-command-xyz <input >out 2>err
status=$?
set -e
if [ "$status" -ne 127 ]; then
    fail 'missing command maps to status 127'
fi
printf 'ok: missing command maps to status 127\n'

printf "'a b' c\n" >input
printf 'a b c\n' >expected
"$UTIL" echo <input >out 2>err
check_file 'quoted input keeps spaces' expected out

printf 'a\\ b\n' >input
printf 'a b\n' >expected
"$UTIL" echo <input >out 2>err
check_file 'backslash escapes a blank' expected out

printf '\n' >input
printf '[]\n' >expected
"$UTIL" -I {} echo '[{}]' <input >out 2>err
check_file '-I handles an empty line' expected out
