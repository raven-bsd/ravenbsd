#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Quoting and comment delimiter behaviour cases.

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

cat >"$tmp/quoted.m4" <<'EOF'
define(`x',`X')dnl
`x'
EOF
check 'quoted text is literal' 'x' "$tmp/quoted.m4"

cat >"$tmp/nested.m4" <<'EOF'
define(`q',`Q')dnl
`a `q' b'
EOF
check 'nested quotes preserve inner delimiters' 'a `q'\'' b' "$tmp/nested.m4"

cat >"$tmp/changequote.m4" <<'EOF'
changequote(`[',`]')dnl
define([y],[Y])dnl
y
EOF
check 'changequote' 'Y' "$tmp/changequote.m4"

cat >"$tmp/reset.m4" <<'EOF'
changequote(`[',`]')dnl
define([y],[Y])dnl
changequote
y
EOF
check 'changequote resets defaults' '
Y' "$tmp/reset.m4"

cat >"$tmp/changecom.m4" <<'EOF'
changecom(`/*',`*/')dnl
define(`x',`X')dnl
/* x */x
EOF
check 'changecom' '/* x */X' "$tmp/changecom.m4"

cat >"$tmp/comments-off.m4" <<'EOF'
define(`x',`X')dnl
changecom()dnl
# x
EOF
check 'changecom with an empty argument disables comments' '# X' "$tmp/comments-off.m4"

cat >"$tmp/comments-bare.m4" <<'EOF'
define(`x',`X')dnl
changecom
dnl
# x
EOF
check 'changecom without arguments disables comments' '
# X' "$tmp/comments-bare.m4"
