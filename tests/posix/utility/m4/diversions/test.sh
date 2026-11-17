#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# divert/undivert behaviour cases.

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

cat >"$tmp/one.m4" <<'EOF'
divert(1)one
divert(0)zero
undivert(1)dnl
EOF
check 'divert and named undivert' 'zero
one' "$tmp/one.m4"

cat >"$tmp/all.m4" <<'EOF'
divert(2)two
divert(1)one
divert(0)zero
EOF
check 'remaining diversions flush in numerical order' 'zero
one
two' "$tmp/all.m4"

cat >"$tmp/order.m4" <<'EOF'
divert(1)one
divert(2)two
divert(0)zero
undivert(2,1)dnl
EOF
check 'undivert uses the argument order' 'zero
two
one' "$tmp/order.m4"

cat >"$tmp/negative.m4" <<'EOF'
a divert(-1)hidden
divert(0)b
EOF
check 'negative diversion discards output' 'a b' "$tmp/negative.m4"

cat >"$tmp/nested.m4" <<'EOF'
divert(1)one
divert(2)dnl
undivert(1)dnl
divert(0)dnl
undivert(2)dnl
EOF
check 'undivert into another diversion' 'one' "$tmp/nested.m4"
