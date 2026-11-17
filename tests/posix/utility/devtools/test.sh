#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# cflow, ctags and cxref cases on one fixed C fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo build -q -p cflow -p ctags -p cxref
cflow_bin="$root/target/debug/cflow"
ctags_bin="$root/target/debug/ctags"
cxref_bin="$root/target/debug/cxref"
for binary in "$cflow_bin" "$ctags_bin" "$cxref_bin"; do
    [ -x "$binary" ] || {
        echo "devtools test: missing $binary" >&2
        exit 1
    }
done

work="$root/build/posix/utility/devtools"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

fixture=tests/posix/utility/devtools/fixture.c

fail() {
    echo "devtools test: FAIL $*" >&2
    exit 1
}

line_is() {
    file=$1
    expected=$2
    grep -qx "$expected" "$file" || fail "$file: missing \`$expected\`"
}

"$ctags_bin" -f "$work/tags" "$fixture" || fail "ctags"
[ -s "$work/tags" ] || fail "empty tags"
first=$(head -n 1 "$work/tags")
[ "$first" = "LIMIT	$fixture	4" ] || fail "tags order: $first"
for entry in \
    "add	$fixture	19" \
    "color	$fixture	11" \
    "global_count	$fixture	13" \
    "helper	$fixture	15" \
    "main	$fixture	23" \
    "point	$fixture	6"
do
    grep -qx "$entry" "$work/tags" || fail "tags: missing \`$entry\`"
done

"$ctags_bin" -x "$fixture" >"$work/tags-x" || fail "ctags -x"
awk -F '\t' -v file="$fixture" '$1=="main" && $2=="23" && $3==file {found=1} END {exit !found}' \
    "$work/tags-x" || fail "ctags -x main"

"$cflow_bin" "$fixture" >"$work/cflow" || fail "cflow"
line_is "$work/cflow" "main: 23"
line_is "$work/cflow" "    add: 19"
line_is "$work/cflow" "    helper: 15"

"$cflow_bin" -r "$fixture" >"$work/cflow-reverse" || fail "cflow -r"
line_is "$work/cflow-reverse" "helper: 15"
line_is "$work/cflow-reverse" "    main: 23"
line_is "$work/cflow-reverse" "add: 19"

"$cflow_bin" -o "$work/cflow-out" "$fixture" || fail "cflow -o"
cmp -s "$work/cflow" "$work/cflow-out" || fail "cflow -o differs"

"$cxref_bin" "$fixture" >"$work/cxref" || fail "cxref"
for entry in \
    "LIMIT	macro	$fixture:4	$fixture:16" \
    "add	function	$fixture:19	$fixture:24" \
    "color	enum	$fixture:11	-" \
    "global_count	global	$fixture:13	$fixture:26" \
    "helper	function	$fixture:15	$fixture:25" \
    "main	function	$fixture:23	-" \
    "point	struct	$fixture:6	-"
do
    grep -qx "$entry" "$work/cxref" || fail "cxref: missing \`$entry\`"
done

"$cxref_bin" -x "$fixture" >"$work/cxref-x" || fail "cxref -x"
cmp -s "$work/cxref" "$work/cxref-x" || fail "cxref -x differs"
"$cxref_bin" -o "$work/cxref-out" "$fixture" || fail "cxref -o"
cmp -s "$work/cxref" "$work/cxref-out" || fail "cxref -o differs"

if "$ctags_bin" -q "$fixture" >/dev/null 2>&1; then
    fail "ctags accepted an unknown option"
fi
if "$cflow_bin" -q "$fixture" >/dev/null 2>&1; then
    fail "cflow accepted an unknown option"
fi
if "$cxref_bin" -q "$fixture" >/dev/null 2>&1; then
    fail "cxref accepted an unknown option"
fi
if "$ctags_bin" >/dev/null 2>&1; then
    fail "ctags accepted no input"
fi
if "$cflow_bin" >/dev/null 2>&1; then
    fail "cflow accepted no input"
fi
if "$cxref_bin" >/dev/null 2>&1; then
    fail "cxref accepted no input"
fi
if "$ctags_bin" -f "$work/missing-tags" "$work/no-such.c" >/dev/null 2>&1; then
    fail "ctags accepted a missing file"
fi

echo "devtools: ok"
