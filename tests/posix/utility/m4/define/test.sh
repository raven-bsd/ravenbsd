#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Define/undefine/defn/pushdef/popdef/dnl behaviour cases.

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

cat >"$tmp/define.m4" <<'EOF'
define(`hello',`world')dnl
hello
EOF
check 'define and expand' 'world' "$tmp/define.m4"

cat >"$tmp/arguments.m4" <<'EOF'
define(`pair',`$1:$2:$#:$*')dnl
pair(`a',`b')
EOF
check 'positional arguments' 'a:b:2:a,b' "$tmp/arguments.m4"

cat >"$tmp/pushdef.m4" <<'EOF'
define(`x',`one')pushdef(`x',`two')dnl
x popdef(`x')x
EOF
check 'pushdef and popdef' 'two one' "$tmp/pushdef.m4"

cat >"$tmp/undefine.m4" <<'EOF'
define(`x',`X')undefine(`x')dnl
x
EOF
check 'undefine' 'x' "$tmp/undefine.m4"

cat >"$tmp/defn.m4" <<'EOF'
define(`a',`A')dnl
defn(`a')
EOF
check 'defn returns the definition' 'A' "$tmp/defn.m4"

cat >"$tmp/dnl.m4" <<'EOF'
left dnl
right
EOF
check 'dnl discards the newline' 'left right' "$tmp/dnl.m4"

cat >"$tmp/dnl-expansion.m4" <<'EOF'
define(`x',`dnl')dnl
x ignored
next
EOF
check 'dnl inside a replacement skips the caller line' 'next' "$tmp/dnl-expansion.m4"

cat >"$tmp/argument-expansion.m4" <<'EOF'
define(`g',`x,y')dnl
define(`f',`[$1][$2][$3]')dnl
f(1,g(2))
EOF
check 'argument expansion contributes commas' '[1][x][y]' "$tmp/argument-expansion.m4"
