#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host test: every cases.tsv row has a case file whose skip text lists exactly the row's HARD
# PGs, and every libc-*.toml case file has a row. This is the C73 4.6.2 consistency check.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
tsv=$root/tests/libc/target/cases.tsv
cases=$root/tests/qtest/cases
tmp=${TMPDIR:-/tmp}/rl-cases-test-$$
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp"

passed=0
failed=0
ok() { passed=$((passed + 1)); }
bad() { failed=$((failed + 1)); printf 'cases-consistency: FAIL %s\n' "$1" >&2; }

while IFS='	' read -r name hard soft binaries link; do
    case $name in
        '' | '#'* | case) continue ;;
    esac
    file=$cases/libc-$name.toml
    if [ ! -f "$file" ]; then
        bad "row $name has no $file"
        continue
    fi
    ok
    skip=$(sed -n 's/^skip = "needs \(.*\)"$/\1/p' "$file")
    if [ -z "$skip" ]; then
        bad "$name has no skip reason"
        continue
    fi
    printf '%s\n' "$skip" | tr ' ' '\n' | sed 's/^PG-//' | sed '/^$/d' | sort -n > "$tmp/skip"
    printf '%s\n' "$hard" | tr ' ' '\n' | sed '/^$/d' | sort -n > "$tmp/hard"
    if cmp -s "$tmp/skip" "$tmp/hard"; then
        ok
    else
        bad "$name skip does not match the HARD column"
    fi
    if grep -qx "name = \"libc-$name\"" "$file"; then
        ok
    else
        bad "$name case name mismatch"
    fi
    if grep -Fq "sh tests/libc/target/build.sh $name x86_64" "$file"; then
        ok
    else
        bad "$name pre does not call build.sh $name"
    fi
    if grep -q '^\[\[expect\]\]$' "$file"; then
        ok
    else
        bad "$name has no expectations"
    fi
done < "$tsv"

for file in "$cases"/libc-*.toml; do
    [ -f "$file" ] || continue
    base=${file##*/}
    name=${base#libc-}
    name=${name%.toml}
    if awk -F '	' -v want="$name" '$0 !~ /^#/ && $1 == want { found = 1 } END { exit found ? 0 : 1 }' "$tsv"; then
        ok
    else
        bad "case file $base has no cases.tsv row"
    fi
done

printf 'cases-consistency: %s passed, %s failed\n' "$passed" "$failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
