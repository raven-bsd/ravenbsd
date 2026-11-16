#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Explicit unsupported and malformed-input behaviour cases.

set -eu
: "${UTIL:?UTIL must point to the host raven-m4 binary}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

expect_failure() {
    name=$1
    script=$2
    pattern=$3
    if "$UTIL" "$script" >"$tmp/out" 2>"$tmp/err"; then
        printf 'FAIL: %s should fail\n' "$name" >&2
        exit 1
    fi
    if [ -s "$tmp/out" ]; then
        printf 'FAIL: %s wrote output\n' "$name" >&2
        exit 1
    fi
    if [ -n "$pattern" ] && ! grep -q "$pattern" "$tmp/err"; then
        printf 'FAIL: %s diagnostic is absent\n' "$name" >&2
        cat "$tmp/err" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

cat >"$tmp/syscmd.m4" <<'EOF'
syscmd(`true')
EOF
expect_failure 'syscmd is unsupported' "$tmp/syscmd.m4" 'syscmd: unsupported macro'

cat >"$tmp/esyscmd.m4" <<'EOF'
esyscmd(`true')
EOF
expect_failure 'esyscmd is unsupported' "$tmp/esyscmd.m4" 'esyscmd: unsupported macro'

cat >"$tmp/quote.m4" <<'EOF'
`unterminated
EOF
expect_failure 'unterminated quote' "$tmp/quote.m4" ''

cat >"$tmp/call.m4" <<'EOF'
define(`x',`X')dnl
x(
EOF
expect_failure 'unterminated macro call' "$tmp/call.m4" ''

cat >"$tmp/empty-name.m4" <<'EOF'
define(,`X')
EOF
expect_failure 'empty define name' "$tmp/empty-name.m4" ''
