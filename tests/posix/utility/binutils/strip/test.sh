#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `strip` utility.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
target=$root/build/posix-utility/binutils-host
if [ -z "${NM:-}" ] || [ -z "${STRIP:-}" ]; then
    printf '==> strip: host build (%s)\n' "$target"
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p nm -p strip
    ) || {
        printf 'strip test: host build failed\n' >&2
        exit 1
    }
fi
NM=${NM:-$target/debug/nm}
STRIP=${STRIP:-$target/debug/strip}
if [ ! -x "$NM" ] || [ ! -x "$STRIP" ]; then
    printf 'strip test: missing host binaries\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

cd "$tmp"
cp "$NM" fixture
chmod +x fixture

"$NM" fixture >before 2>err
[ -s before ] || fail 'nm before strip is empty'
grep -q ' T ' before || fail 'nm before strip has no text symbol'
printf 'ok: nm before strip lists symbols\n'

"$STRIP" fixture || fail 'strip failed on a real ELF'
printf 'ok: strip removed symbol and debug sections\n'

set +e
"$tmp/fixture" >run.out 2>run.err
status=$?
set -e
[ "$status" -eq 2 ] || fail "stripped binary exited $status, not the usage status 2"
grep -q 'usage: nm' run.err || fail 'stripped binary printed no usage text'
printf 'ok: stripped binary still runs\n'

"$NM" fixture >after 2>after.err
[ ! -s after ] || fail 'nm after strip still lists symbols'
grep -q 'no symbols' after.err || fail 'nm after strip did not report no symbols'
printf 'ok: nm after strip reports no symbols\n'

cp "$NM" debug-fixture
chmod +x debug-fixture
"$NM" debug-fixture >debug.before
grep -q ' T ' debug.before || fail 'debug fixture has no text symbol'
"$STRIP" -g debug-fixture || fail 'strip -g failed'
"$NM" debug-fixture >debug.after 2>debug.err
[ -s debug.after ] || fail 'strip -g removed the symbol table'
grep -q ' T ' debug.after || fail 'strip -g lost text symbols'
printf 'ok: strip -g keeps the symbol table\n'

cp "$NM" input
cp "$NM" keep
"$STRIP" -o output input || fail 'strip -o failed'
cmp -s input keep || fail 'strip -o modified the input file'
[ -x output ] || fail 'strip -o output is not executable'
"$NM" output >output.syms 2>output.err
[ ! -s output.syms ] || fail 'strip -o output still lists symbols'
grep -q 'no symbols' output.err || fail 'strip -o output did not report no symbols'
printf 'ok: strip -o leaves the input untouched\n'

set +e
"$tmp/output" >output.run.out 2>output.run.err
status=$?
set -e
[ "$status" -eq 2 ] || fail "strip -o output exited $status, not the usage status 2"
printf 'ok: strip -o output still runs\n'

cp "$NM" all-fixture
chmod +x all-fixture
"$STRIP" -s all-fixture || fail 'strip -s failed'
"$NM" all-fixture >all.syms 2>all.err
[ ! -s all.syms ] || fail 'strip -s output still lists symbols'
printf 'ok: strip -s removes all symbols\n'

printf 'not an ELF file\n' >bad
if "$STRIP" bad >out 2>err; then
    fail 'malformed input was accepted'
fi
grep -q 'not a supported ELF32/ELF64 file' err || fail 'malformed diagnostic missing'
printf 'ok: strip rejects malformed input\n'

if "$STRIP" >out 2>err; then
    fail 'missing operand was accepted'
fi
grep -q 'usage: strip' err || fail 'usage diagnostic missing'
printf 'ok: missing operand is a usage error\n'

if "$STRIP" -o out input keep >out 2>err; then
    fail '-o with multiple inputs was accepted'
fi
grep -q 'single input file' err || fail '-o multiple-input diagnostic missing'
printf 'ok: -o rejects multiple inputs\n'
