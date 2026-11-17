#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# incr/decr/eval behaviour cases.

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

cat >"$tmp/incr.m4" <<'EOF'
incr(41) decr(41)
EOF
check 'incr and decr' '42 40' "$tmp/incr.m4"

cat >"$tmp/precedence.m4" <<'EOF'
eval(2+3*4)
EOF
check 'eval precedence' '14' "$tmp/precedence.m4"

cat >"$tmp/parentheses.m4" <<'EOF'
eval((2+3)*4) eval(2**10)
EOF
check 'eval parentheses and power' '20 1024' "$tmp/parentheses.m4"

cat >"$tmp/logical.m4" <<'EOF'
eval(3<5) eval(3==4) eval(1&&0) eval(1||0)
EOF
check 'eval comparisons and logical operators' '1 0 0 1' "$tmp/logical.m4"

cat >"$tmp/radix.m4" <<'EOF'
eval(010) eval(0x10) eval(0b101)
EOF
check 'eval octal, hexadecimal and binary numbers' '8 16 5' "$tmp/radix.m4"

cat >"$tmp/modulo.m4" <<'EOF'
eval(17%5)
EOF
check 'eval remainder' '2' "$tmp/modulo.m4"

cat >"$tmp/divide-zero.m4" <<'EOF'
eval(1/0)
EOF
if "$UTIL" "$tmp/divide-zero.m4" >"$tmp/out" 2>"$tmp/err"; then
    printf 'FAIL: division by zero should fail\n' >&2
    exit 1
fi
if [ -s "$tmp/out" ]; then
    printf 'FAIL: division by zero wrote output\n' >&2
    exit 1
fi
printf 'ok: eval division by zero is an error\n'

cat >"$tmp/invalid.m4" <<'EOF'
eval(1+)
EOF
if "$UTIL" "$tmp/invalid.m4" >"$tmp/out" 2>"$tmp/err"; then
    printf 'FAIL: invalid expression should fail\n' >&2
    exit 1
fi
printf 'ok: eval rejects an incomplete expression\n'
