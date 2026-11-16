#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Command-line, stdin and file-operand behaviour cases.

set -eu
: "${UTIL:?UTIL must point to the host raven-m4 binary}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/inc"

check() {
    name=$1
    expected=$2
    shift 2
    actual=$("$@")
    if [ "$actual" != "$expected" ]; then
        printf 'FAIL: %s\nexpected: [%s]\nactual:   [%s]\n' "$name" "$expected" "$actual" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

cat >"$tmp/hello.m4" <<'EOF'
define(`hello',`world')dnl
hello
EOF
actual=$("$UTIL" <"$tmp/hello.m4")
if [ "$actual" != "world" ]; then
    printf 'FAIL: standard input\nexpected: [world]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: standard input\n'

actual=$("$UTIL" - <"$tmp/hello.m4")
if [ "$actual" != "world" ]; then
    printf 'FAIL: dash operand reads standard input\nexpected: [world]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: dash operand reads standard input\n'

check 'file operand' 'world' "$UTIL" "$tmp/hello.m4"

cat >"$tmp/defs.m4" <<'EOF'
define(`shared',`value')dnl
EOF
cat >"$tmp/use.m4" <<'EOF'
shared
EOF
check 'definitions persist across file operands' 'value' "$UTIL" "$tmp/defs.m4" "$tmp/use.m4"

cat >"$tmp/name.m4" <<'EOF'
name
EOF
check '-D attached' 'bo' "$UTIL" -Dname=bo "$tmp/name.m4"
check '-D separate' 'bo' "$UTIL" -D name=bo "$tmp/name.m4"

cat >"$tmp/defined.m4" <<'EOF'
ifdef(`name',`yes',`no')
EOF
check '-D without a value' 'yes' "$UTIL" -Dname "$tmp/defined.m4"

check '-U attached' 'name' "$UTIL" -Dname=bo -Uname "$tmp/name.m4"
check '-U separate' 'name' "$UTIL" -Dname=bo -U name "$tmp/name.m4"
check '-D then -U order' 'name' "$UTIL" -Dname=bo -Uname "$tmp/name.m4"
check '-U then -D order' 'bo' "$UTIL" -Uname -Dname=bo "$tmp/name.m4"

cat >"$tmp/inc/defs.m4" <<'EOF'
define(`inc',`INCLUDED')dnl
EOF
cat >"$tmp/included.m4" <<'EOF'
include(`defs.m4')dnl
inc
EOF
check '-I attached' 'INCLUDED' "$UTIL" "-I$tmp/inc" "$tmp/included.m4"

cat >"$tmp/-input.m4" <<'EOF'
dash-file
EOF
actual=$(cd "$tmp" && "$UTIL" -- -input.m4)
if [ "$actual" != "dash-file" ]; then
    printf 'FAIL: -- permits a leading-dash operand\nexpected: [dash-file]\nactual:   [%s]\n' "$actual" >&2
    exit 1
fi
printf 'ok: -- permits a leading-dash operand\n'

if "$UTIL" -Q >"$tmp/out" 2>"$tmp/err"; then
    printf 'FAIL: an unknown option should fail\n' >&2
    exit 1
fi
printf 'ok: unknown option is a usage error\n'
