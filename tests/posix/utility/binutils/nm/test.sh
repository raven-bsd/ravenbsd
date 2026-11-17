#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `nm` utility.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
target=$root/build/posix-utility/binutils-host
if [ -z "${NM:-}" ]; then
    printf '==> nm: host build (%s)\n' "$target"
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p nm
    ) || {
        printf 'nm test: host build failed\n' >&2
        exit 1
    }
    NM=$target/debug/nm
fi
if [ ! -x "$NM" ]; then
    printf 'nm test: missing host binary\n' >&2
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

"$NM" fixture >syms 2>err
[ -s syms ] || fail 'default listing is empty'
grep -q ' T ' syms || fail 'default listing has no text symbol'
grep -q ' U ' syms || fail 'default listing has no undefined symbol'
[ ! -s err ] || fail 'default listing wrote an error'
printf 'ok: default listing has text and undefined symbols\n'

"$NM" -u fixture >undef 2>err
[ -s undef ] || fail '-u listing is empty'
if grep -q ' T ' undef; then
    fail '-u included a defined text symbol'
fi
grep -q ' U ' undef || fail '-u has no U row'
printf 'ok: -u keeps only undefined symbols\n'

"$NM" -g fixture >globals 2>err
[ -s globals ] || fail '-g listing is empty'
if grep -q ' t ' globals; then
    fail '-g included a local text symbol'
fi
printf 'ok: -g drops local symbols\n'

"$NM" -n fixture >numeric 2>err
[ -s numeric ] || fail '-n listing is empty'
if cmp -s numeric syms; then
    fail '-n did not change the order'
fi
printf 'ok: -n orders by value\n'

"$NM" -p fixture >table 2>err
[ -s table ] || fail '-p listing is empty'
[ "$(wc -l <table)" -eq "$(wc -l <syms)" ] || fail '-p changed the entry count'
printf 'ok: -p preserves the symbol table order\n'

"$NM" -a fixture >all 2>err
[ -s all ] || fail '-a listing is empty'
[ "$(wc -l <all)" -ge "$(wc -l <syms)" ] || fail '-a lost symbols'
printf 'ok: -a includes additional symbols\n'

printf 'not an ELF file\n' >bad
if "$NM" bad >out 2>err; then
    fail 'malformed input was accepted'
fi
[ ! -s out ] || fail 'malformed input wrote stdout'
grep -q 'not a supported ELF32/ELF64 object' err || fail 'malformed diagnostic missing'
printf 'ok: malformed input is rejected\n'

if "$NM" >out 2>err; then
    fail 'missing operand was accepted'
fi
grep -q 'usage: nm' err || fail 'usage diagnostic missing'
printf 'ok: missing operand is a usage error\n'
