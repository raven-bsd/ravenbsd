#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# include and -I behaviour cases.

set -eu
: "${UTIL:?UTIL must point to the host raven-m4 binary}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/inc"

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

cat >"$tmp/inc/defs.m4" <<'EOF'
define(`from_include',`INCLUDED')dnl
EOF
cat >"$tmp/main.m4" <<'EOF'
include(`defs.m4')dnl
from_include
EOF
actual=$("$UTIL" -I "$tmp/inc" "$tmp/main.m4")
if [ "$actual" != "INCLUDED" ]; then
    printf 'FAIL: -I include search path\nexpected: [INCLUDED]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: -I include search path\n'

cat >"$tmp/main2.m4" <<'EOF'
include(`defs.m4')dnl
from_include
EOF
actual=$(cd "$tmp/inc" && "$UTIL" "$tmp/main2.m4")
if [ "$actual" != "INCLUDED" ]; then
    printf 'FAIL: include from the current directory\nexpected: [INCLUDED]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: include from the current directory\n'

cat >"$tmp/inc/outer.m4" <<'EOF'
include(`defs.m4')dnl
EOF
cat >"$tmp/nested.m4" <<'EOF'
include(`outer.m4')dnl
from_include
EOF
actual=$("$UTIL" -I "$tmp/inc" "$tmp/nested.m4")
if [ "$actual" != "INCLUDED" ]; then
    printf 'FAIL: nested include\nexpected: [INCLUDED]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: nested include\n'

cat >"$tmp/inc/value.m4" <<'EOF'
VALUE
EOF
cat >"$tmp/argument.m4" <<'EOF'
define(`x', include(`value.m4'))dnl
x
EOF
actual=$("$UTIL" -I "$tmp/inc" "$tmp/argument.m4")
if [ "$actual" != "VALUE" ]; then
    printf 'FAIL: include supplies argument text\nexpected: [VALUE]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: include supplies argument text\n'

cat >"$tmp/missing.m4" <<'EOF'
include(`missing.m4')
EOF
if "$UTIL" "$tmp/missing.m4" >"$tmp/out" 2>"$tmp/err"; then
    printf 'FAIL: a missing include should fail\n' >&2
    exit 1
fi
if ! grep -q 'cannot open include file' "$tmp/err"; then
    printf 'FAIL: missing include diagnostic is absent\n' >&2
    cat "$tmp/err" >&2
    exit 1
fi
printf 'ok: missing include is an error\n'
