#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# ifdef and ifelse behaviour cases.

set -eu
: "${UTIL:?UTIL must point to the host raven-m4 binary}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

check() {
    name=$1
    expected=$2
    script=$3
    actual=$("$UTIL" "$script")
    if [ "$actual" != "$expected" ]; then
        printf 'FAIL: %s\nexpected: [%s]\nactual:   [%s]\n' "$name" "$expected" "$actual" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

cat >"$tmp/ifdef-true.m4" <<'EOF'
define(`a',`A')dnl
ifdef(`a',`yes',`no')
EOF
check 'ifdef defined' 'yes' "$tmp/ifdef-true.m4"

cat >"$tmp/ifdef-false.m4" <<'EOF'
ifdef(`missing',`yes',`no')
EOF
check 'ifdef undefined' 'no' "$tmp/ifdef-false.m4"

cat >"$tmp/ifelse-equal.m4" <<'EOF'
ifelse(`a',`a',`same',`diff')
EOF
check 'ifelse equal' 'same' "$tmp/ifelse-equal.m4"

cat >"$tmp/ifelse-diff.m4" <<'EOF'
ifelse(`a',`b',`same',`diff')
EOF
check 'ifelse unequal' 'diff' "$tmp/ifelse-diff.m4"

cat >"$tmp/ifelse-empty.m4" <<'EOF'
x:ifelse(`a',`b',`same'):y
EOF
check 'ifelse without an else branch' 'x::y' "$tmp/ifelse-empty.m4"

cat >"$tmp/ifelse-chain.m4" <<'EOF'
ifelse(`a',`b',`one',`c',`c',`two',`three')
EOF
check 'ifelse chain' 'two' "$tmp/ifelse-chain.m4"

cat >"$tmp/nested-false.m4" <<'EOF'
ifdef(`x',`define(`q',`Q')',`')dnl
q
EOF
check 'quoted define in an unselected branch stays literal' 'q' "$tmp/nested-false.m4"

cat >"$tmp/nested-true.m4" <<'EOF'
ifdef(`define',`define(`q',`Q')',`')dnl
q
EOF
check 'quoted define in the selected branch is rescanned' 'Q' "$tmp/nested-true.m4"
