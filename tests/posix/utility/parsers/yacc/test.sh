#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P42 yacc + lex behaviour: build the calculator, run precedence/associativity and error recovery.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../../../.." && pwd)
target=$root/build/posix-utility/parsers

if [ -z "${LEX:-}" ] || [ -z "${YACC:-}" ]; then
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p lex -p yacc
    )
    LEX=${LEX:-$target/debug/lex}
    YACC=${YACC:-$target/debug/yacc}
fi
if [ ! -x "$LEX" ] || [ ! -x "$YACC" ]; then
    printf 'yacc test: missing host binary\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$here/calc.y" "$here/calc.l" "$here/expected.txt" "$tmp/"
cd "$tmp"

"$YACC" -v -d calc.y 2> yacc.log
if ! grep -q '0 shift/reduce' yacc.log; then
    printf 'yacc test: unexpected conflicts\n' >&2
    cat yacc.log >&2
    exit 1
fi
"$LEX" -t calc.l > lex.yy.c
cc -Wall -Wextra -Werror -o calc y.tab.c lex.yy.c

printf '2+3*4\n(2+3)*4\n-2*3\n2-3-4\n8/2*2\n' | ./calc > actual 2> error
if ! cmp -s expected.txt actual; then
    printf 'yacc test: calculator output mismatch\n' >&2
    printf '%s\n' '--- expected ---' >&2
    cat expected.txt >&2
    printf '%s\n' '--- actual ---' >&2
    cat actual >&2
    exit 1
fi
if [ -s error ]; then
    printf 'yacc test: unexpected stderr\n' >&2
    cat error >&2
    exit 1
fi
printf 'ok: precedence, associativity, parentheses and unary minus\n'

printf '2+\n3+4\n' | ./calc > recovered.out 2> recovered.err
if ! grep -q '^syntax error$' recovered.err; then
    printf 'yacc test: missing syntax error\n' >&2
    cat recovered.err >&2
    exit 1
fi
if ! grep -q '^7$' recovered.out; then
    printf 'yacc test: error recovery did not resume\n' >&2
    cat recovered.out >&2
    exit 1
fi
printf 'ok: error-token recovery\n'
